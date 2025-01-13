use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use std::env;
use tracing::info;

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
                .route("/", web::get().to(handle_get))
                .route("/", web::post().to(handle_post))
        })
        .bind(server_address)?
        .run()
        .await
    })?;

    Ok(())
}

async fn handle_get(_req: HttpRequest) -> impl Responder {
    info!("Handling GET request");
    HttpResponse::Ok().body("GET request received")
}

async fn handle_post(_req: HttpRequest) -> impl Responder {
    info!("Handling POST request");
    HttpResponse::Ok().body("POST request received")
}
