use anyhow::Result;
use axum::extract::{ConnectInfo, Json};
use axum::http::{HeaderMap, StatusCode};
use axum::{routing::post, Router};
use axum_extra::extract::TypedHeader;
use axum_extra::headers::{authorization::Bearer, Authorization};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::{collections::HashSet, sync::Mutex};
use tokio::net::TcpListener;
use tracing::{info, warn};

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
    priority: Option<String>,
}

/// Represents the request payload for syncing seen articles.
#[derive(Deserialize)]
struct SyncSeenArticlesRequest {
    seen_articles: Vec<String>,
}

/// Represents the response payload for unseen articles.
#[derive(Serialize)]
struct SyncSeenArticlesResponse {
    unseen_articles: Vec<String>,
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
    let mut topics = std::env::var("TOPICS")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split(':').next().map(str::trim).map(String::from))
        .collect::<HashSet<String>>();
    topics.insert("Alert: Direct".to_string());
    topics.insert("Alert: Near".to_string());
    topics.insert("Test".to_string());
    topics
});

/// Main application loop, setting up and running the Axum-based API server.
pub async fn app_api_loop() -> Result<()> {
    let app = Router::new()
        .route("/status", post(status_check))
        .route("/authenticate", post(authenticate))
        .route("/subscribe", post(subscribe_to_topic))
        .route("/unsubscribe", post(unsubscribe_from_topic))
        .route("/articles/sync", post(sync_seen_articles));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{}", port);

    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Server running on http://{}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();

    Ok(())
}

// Helper to extract proxied client address.
fn get_client_ip(headers: &HeaderMap, socket_addr: &SocketAddr) -> String {
    let ip_from_headers = headers
        .get("CF-Connecting-IP")
        .and_then(|hv| hv.to_str().ok())
        .or_else(|| {
            headers
                .get("X-Forwarded-For")
                .and_then(|hv| hv.to_str().ok())
                .and_then(|s| s.split(',').next())
        });

    if let Some(ip_str) = ip_from_headers {
        // Try to parse as IPv4 first
        if let Ok(ip) = ip_str.parse::<std::net::Ipv4Addr>() {
            return ip.to_string();
        }
        // If IPv4 parsing fails, return the original string (could be IPv6)
        return ip_str.to_string();
    }

    // Fallback to socket address
    match socket_addr.ip() {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => ip.to_string(),
    }
}

/// Handles authentication requests by validating the device ID and returning a JWT token.
async fn authenticate(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<AuthRequest>,
) -> Json<AuthResponse> {
    info!(
        "Authenticating device_id: {} from IP: {}",
        payload.device_id,
        addr.ip()
    );

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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    info!(
        "app::api subscribe_to_topic request from IP {} for topic: {}",
        addr.ip(),
        payload.topic
    );
    let token = auth_header.token();
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|e| {
            warn!(
                "app::api subscribe_to_topic JWT validation failed: {:#?}",
                e
            );
            StatusCode::UNAUTHORIZED
        })?;
    let device_id = claims.claims.sub;
    info!(
        "app::api subscribe_to_topic validated JWT for device_id: {}",
        device_id
    );
    // Validate the provided topic
    if !VALID_TOPICS.contains(&payload.topic) {
        warn!(
            "app::api subscribe_to_topic invalid topic: {}",
            payload.topic
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    // Get database instance and subscribe the device
    info!(
        "app::api subscribe_to_topic subscribing device_id: {} to topic: {}",
        device_id, payload.topic
    );
    let db: &Database = Database::instance().await;
    match db
        .subscribe_to_topic(&device_id, &payload.topic, payload.priority.as_deref())
        .await
    {
        Ok(_) => {
            info!(
                "app::api subscribe_to_topic successfully subscribed device_id: {} to topic: {}",
                device_id, payload.topic
            );
            Ok(StatusCode::OK) // Successfully subscribed
        }
        Err(sqlx::Error::Database(err)) if err.message().contains("UNIQUE constraint failed") => {
            warn!("app::api subscribe_to_topic subscription already exists for device_id: {} and topic: {}", device_id, payload.topic);
            Ok(StatusCode::CONFLICT) // The subscription already exists
        }
        Err(e) => {
            warn!("app::api subscribe_to_topic unexpected error: {:#?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR) // Generic error for other cases
        }
    }
}

/// Unsubscribes a device from a topic after validating the JWT and topic validity.
async fn unsubscribe_from_topic(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    info!(
        "app::api unsusbcribe_from_topic request from IP {} for topic: {}",
        addr.ip(),
        payload.topic
    );
    let token = auth_header.token();

    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|e| {
            warn!(
                "app::api unsubscribe_from_topic JWT validation failed: {:#?}",
                e
            );
            StatusCode::UNAUTHORIZED
        })?;

    let device_id = claims.claims.sub;
    info!(
        "app::api unsubscribe_from_topic validated JWT for device_id: {}",
        device_id
    );

    // Validate the provided topic
    if !VALID_TOPICS.contains(&payload.topic) {
        warn!(
            "app::api unsubscribe_from_topic invalid topic: {}",
            payload.topic
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get database instance and unsubscribe the device
    info!(
        "app::api unsubscribe_from_topic unsubscribing device_id: {} from topic: {}",
        device_id, payload.topic
    );
    let db: &Database = Database::instance().await;
    match db.unsubscribe_from_topic(&device_id, &payload.topic).await {
        Ok(true) => {
            info!("app::api unsubscribe_from_topic successfully unsubscribed device_id: {} from topic: {}", device_id, payload.topic);
            Ok(StatusCode::OK) // Successfully unsubscribed
        }
        Ok(false) => {
            warn!("app::api unsubscribe_from_topic no subscription found for device_id: {} and topic: {}", device_id, payload.topic);
            Err(StatusCode::NOT_FOUND) // Subscription not found
        }
        Err(e) => {
            warn!("app::api unsubscribe_from_topic unexpected error: {:#?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR) // Generic error for other cases
        }
    }
}

/// Checks the server's status, optionally validating a JWT if provided.
async fn status_check(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth_header: Option<TypedHeader<Authorization<Bearer>>>,
) -> Result<&'static str, StatusCode> {
    info!("app::api status_check request from IP {} ", addr.ip(),);
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

/// Handles syncing seen articles and returning unseen articles.
async fn sync_seen_articles(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<SyncSeenArticlesRequest>,
) -> Result<Json<SyncSeenArticlesResponse>, StatusCode> {
    let client_ip = get_client_ip(&headers, &addr);
    info!("app::api sync_seen_articles request from IP {}", client_ip);
    let token = auth_header.token();

    // Validate JWT and extract claims
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let device_id = claims.claims.sub;
    info!("Syncing seen articles for device_id: {}", device_id);

    let db: &Database = Database::instance().await;

    // Get unseen articles from the database
    let unseen_articles = match db
        .fetch_unseen_articles(&device_id, &payload.seen_articles)
        .await
    {
        Ok(articles) => articles,
        Err(e) => {
            warn!("Error fetching unseen articles: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(Json(SyncSeenArticlesResponse { unseen_articles }))
}
