use anyhow::Result;
use axum::{
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use tokio::net::TcpListener;
use tracing::{info, Level};
use tracing_subscriber;

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

pub async fn app_api_loop() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    // Build the Axum router
    let app = Router::new()
        .route("/status", get(status_check))
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

/// Status check endpoint: Replies with "OK"
async fn status_check() -> &'static str {
    info!("Handling status check GET request");
    "OK"
}

/// Keyless authentication endpoint: Generates a cryptographically secure 64-character token
async fn authenticate() -> Json<AuthResponse> {
    info!("Handling keyless authentication POST request");

    let rng = SystemRandom::new();
    let mut token_bytes = [0u8; 48]; // 48 bytes = 64 characters in Base64
    if rng.fill(&mut token_bytes).is_err() {
        panic!("Failed to generate secure random bytes");
    }

    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&token_bytes);

    Json(AuthResponse { token })
}
