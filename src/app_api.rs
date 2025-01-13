use anyhow::Result;
use axum::{extract::Json, http::StatusCode, routing::post, Router};
use axum_extra::extract::TypedHeader;
use axum_extra::headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Mutex};
use tokio::net::TcpListener;
use tracing::info;

use crate::db::Database;

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Deserialize)]
struct AuthRequest {
    device_id: String,
}

#[derive(Deserialize)]
struct TopicRequest {
    topic: String,
}

static PRIVATE_KEY: Lazy<Mutex<Vec<u8>>> = Lazy::new(|| {
    let rng = SystemRandom::new();
    let mut key_bytes = vec![0u8; 32]; // 256-bit key for HMAC
    rng.fill(&mut key_bytes)
        .expect("Failed to generate secure random bytes");
    Mutex::new(key_bytes)
});

static ENCODING_KEY: Lazy<EncodingKey> = Lazy::new(|| {
    let key = PRIVATE_KEY.lock().unwrap();
    EncodingKey::from_secret(&key)
});

static DECODING_KEY: Lazy<DecodingKey> = Lazy::new(|| {
    let key = PRIVATE_KEY.lock().unwrap();
    DecodingKey::from_secret(&key)
});

static VALID_TOPICS: Lazy<HashSet<String>> = Lazy::new(|| {
    let topics_env = std::env::var("TOPICS").unwrap_or_default();
    topics_env
        .lines()
        .filter_map(|line| line.split(':').next().map(str::trim).map(String::from))
        .collect()
});

pub async fn app_api_loop() -> Result<()> {
    let app = Router::new()
        .route("/status", post(status_check))
        .route("/authenticate", post(authenticate))
        .route("/subscribe", post(subscribe_to_topic))
        .route("/unsubscribe", post(unsubscribe_from_topic));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);

    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Server running on http://{}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn authenticate(Json(payload): Json<AuthRequest>) -> Json<AuthResponse> {
    info!("Authenticating device_id: {}", payload.device_id);

    let claims = Claims {
        sub: payload.device_id.clone(),
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };

    let token = encode(&Header::new(Algorithm::HS256), &claims, &ENCODING_KEY)
        .expect("Failed to encode JWT");

    Json(AuthResponse { token })
}

async fn subscribe_to_topic(
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let token = auth_header.token();
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let device_id = claims.claims.sub;
    if !VALID_TOPICS.contains(&payload.topic) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db: &Database = Database::instance().await;
    db.subscribe_to_topic(&device_id, &payload.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn unsubscribe_from_topic(
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let token = auth_header.token();
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let device_id = claims.claims.sub;
    if !VALID_TOPICS.contains(&payload.topic) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db: &Database = Database::instance().await;
    db.unsubscribe_from_topic(&device_id, &payload.topic)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn status_check(
    auth_header: Option<TypedHeader<Authorization<Bearer>>>,
) -> Result<&'static str, StatusCode> {
    if let Some(TypedHeader(auth_header)) = auth_header {
        let token = auth_header.token();
        if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_ok() {
            info!("Valid JWT provided for status check");
            return Ok("OK");
        } else {
            info!("Invalid JWT provided for status check");
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    info!("No JWT provided for status check");
    Ok("OK")
}
