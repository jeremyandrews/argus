use anyhow::Result;
use axum::{extract::Json, http::StatusCode, routing::post, Router};
use axum_extra::extract::TypedHeader;
use axum_extra::headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::net::TcpListener;
use tracing::info;

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

pub async fn app_api_loop() -> Result<()> {
    // Build the Axum router
    let app = Router::new()
        .route("/status", post(status_check))
        .route("/authenticate", post(authenticate));

    // Determine the port to listen on
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);

    // Bind to the address using Tokio's TcpListener
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Server running on http://{}", addr);

    // Start the server
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

/// Status check endpoint: Replies with "OK" or validates JWT
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

/// Authentication endpoint: Generates a JWT for session management
async fn authenticate(Json(payload): Json<AuthRequest>) -> Json<AuthResponse> {
    info!(
        "Handling authentication POST request for device_id: {}",
        payload.device_id
    );

    let claims = Claims {
        sub: payload.device_id.clone(), // Use the device_id as the subject
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize, // 1 hour expiry
    };

    let token = encode(&Header::new(Algorithm::HS256), &claims, &ENCODING_KEY)
        .expect("Failed to encode JWT");

    Json(AuthResponse { token })
}
