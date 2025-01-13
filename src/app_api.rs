use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use std::env;
use tracing::info;

#[derive(Serialize)]
struct AuthResponse {
    token: String,
}

pub async fn app_api_loop() -> Result<()> {
    // Get the port from environment variables or default to 8080
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    let server_address = format!("0.0.0.0:{}", port);
    info!("Server running on http://{}", server_address);

    // Use `actix_web::rt::System` to start the server in a dedicated runtime
    actix_web::rt::System::new().block_on(async {
        HttpServer::new(|| {
            App::new()
                .route("/status", web::get().to(status_check))
                .route("/authenticate", web::post().to(authenticate))
        })
        .bind(server_address)?
        .run()
        .await
    })?;

    Ok(())
}

/// Status check endpoint: Replies with "OK" to a GET request
async fn status_check(_req: HttpRequest) -> impl Responder {
    info!("Handling status check GET request");
    HttpResponse::Ok().body("OK")
}

/// Keyless authentication endpoint: Generates a cryptographically secure 64-character token
async fn authenticate(_req: HttpRequest) -> impl Responder {
    info!("Handling keyless authentication POST request");

    let rng = SystemRandom::new();
    let mut token_bytes = [0u8; 48]; // 48 bytes = 64 characters in Base64
    if let Err(e) = rng.fill(&mut token_bytes) {
        info!("Failed to generate secure random bytes: {:?}", e);
        return HttpResponse::InternalServerError().body("Failed to generate token");
    }

    // Use the `URL_SAFE_NO_PAD` engine for encoding
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&token_bytes);

    let response = AuthResponse { token };

    HttpResponse::Ok().json(response)
}
