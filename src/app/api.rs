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

/// Represents the response for an authentication request, containing a JWT token.
#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

/// Represents the claims stored in a JWT token.
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String, // Subject (e.g., device ID)
    exp: usize,  // Expiration time (as a timestamp)
}

/// Represents the request payload for authentication, containing a device ID.
#[derive(Deserialize)]
struct AuthRequest {
    device_id: String,
}

/// Represents the request payload for topic subscription and unsubscription.
#[derive(Deserialize)]
struct TopicRequest {
    topic: String,
}

/// Static private key used for encoding and decoding JWT tokens.
static PRIVATE_KEY: Lazy<Mutex<Vec<u8>>> = Lazy::new(|| {
    let rng = SystemRandom::new();
    let mut key_bytes = vec![0u8; 32]; // 256-bit key for HMAC
    rng.fill(&mut key_bytes)
        .expect("Failed to generate secure random bytes");
    Mutex::new(key_bytes)
});

/// Static encoding key for generating JWT tokens.
static ENCODING_KEY: Lazy<EncodingKey> = Lazy::new(|| {
    let key = PRIVATE_KEY.lock().unwrap();
    EncodingKey::from_secret(&key)
});

/// Static decoding key for validating JWT tokens.
static DECODING_KEY: Lazy<DecodingKey> = Lazy::new(|| {
    let key = PRIVATE_KEY.lock().unwrap();
    DecodingKey::from_secret(&key)
});

/// Static set of valid topics parsed from an environment variable.
static VALID_TOPICS: Lazy<HashSet<String>> = Lazy::new(|| {
    let topics_env = std::env::var("TOPICS").unwrap_or_default();
    topics_env
        .lines()
        .filter_map(|line| line.split(':').next().map(str::trim).map(String::from))
        .collect()
});

/// Main application loop, setting up and running the Axum-based API server.
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

/// Handles authentication requests by validating the device ID and returning a JWT token.
async fn authenticate(Json(payload): Json<AuthRequest>) -> Json<AuthResponse> {
    info!("Authenticating device_id: {}", payload.device_id);

    // Basic validation for iOS device token
    if payload.device_id.len() != 64 || !payload.device_id.chars().all(|c| c.is_ascii_hexdigit()) {
        tracing::error!("Invalid iOS device token format: {}", payload.device_id);
        return Json(AuthResponse {
            token: "Invalid device token".to_string(),
        });
    }

    let claims = Claims {
        sub: payload.device_id.clone(),
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };

    let token = encode(&Header::new(Algorithm::HS256), &claims, &ENCODING_KEY)
        .expect("Failed to encode JWT");

    Json(AuthResponse { token })
}

/// Subscribes a device to a topic after validating the JWT and topic validity.
async fn subscribe_to_topic(
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let token = auth_header.token();

    // Validate JWT and extract claims
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let device_id = claims.claims.sub;

    // Validate the provided topic
    if !VALID_TOPICS.contains(&payload.topic) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get database instance and subscribe the device
    let db: &Database = Database::instance().await;
    match db.subscribe_to_topic(&device_id, &payload.topic).await {
        Ok(_) => Ok(StatusCode::OK), // Successfully subscribed
        Err(sqlx::Error::Database(err)) if err.message().contains("UNIQUE constraint failed") => {
            // The subscription already exists
            Ok(StatusCode::CONFLICT)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR), // Generic error for other cases
    }
}

/// Unsubscribes a device from a topic after validating the JWT and topic validity.
async fn unsubscribe_from_topic(
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let token = auth_header.token();

    // Validate JWT and extract claims
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let device_id = claims.claims.sub;

    // Validate the provided topic
    if !VALID_TOPICS.contains(&payload.topic) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get database instance and unsubscribe the device
    let db: &Database = Database::instance().await;
    match db.unsubscribe_from_topic(&device_id, &payload.topic).await {
        Ok(true) => Ok(StatusCode::OK),          // Successfully unsubscribed
        Ok(false) => Err(StatusCode::NOT_FOUND), // Subscription not found
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR), // Generic error for other cases
    }
}

/// Checks the server's status, optionally validating a JWT if provided.
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
