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
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::db::core::Database;
use crate::entity::matching::calculate_entity_similarity;
use crate::vector::search::get_article_entities;
use crate::SubscriptionsResponse;

/// Request for syncing clusters
#[derive(Deserialize)]
struct SyncClustersRequest {
    known_clusters: Vec<ClusterSyncInfo>,
}

/// Simple structure for tracking client's known clusters
#[derive(Deserialize)]
struct ClusterSyncInfo {
    id: i64,
    version: i32, // Based on summary_version
}

/// Response for the cluster sync endpoint
#[derive(Serialize)]
struct SyncClustersResponse {
    updated_clusters: Vec<ClusterData>,
    deleted_clusters: Vec<i64>,
    merged_clusters: Vec<ClusterMergeInfo>,
}

/// Cluster data for API responses
#[derive(Serialize)]
struct ClusterData {
    id: i64,
    version: i32,
    creation_date: String,
    summary: Option<String>,
    article_count: i32,
    importance_score: f64,
    has_timeline: bool,
    articles: Vec<ArticleBrief>,
}

/// Brief article data for cluster listings
#[derive(Serialize)]
pub struct ArticleBrief {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub pub_date: String,
    pub similarity_score: f64,
}

/// Information about merged clusters
#[derive(Serialize)]
struct ClusterMergeInfo {
    original_id: i64,
    merged_into_id: i64,
}

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

/// Request for analyzing match between two articles
#[derive(Deserialize)]
struct ArticleMatchAnalysisRequest {
    source_article_id: i64,
    target_article_id: i64,
}

/// Detailed response about article match analysis
#[derive(Serialize)]
struct ArticleMatchAnalysisResponse {
    source_article_id: i64,
    target_article_id: i64,
    is_self_match: bool,
    vector_similarity: f32,
    entity_similarity: Option<f32>,
    combined_score: f32,
    threshold: f32,
    match_status: bool,
    source_entity_count: usize,
    target_entity_count: usize,
    shared_entity_count: usize,
    shared_primary_entity_count: usize,
    person_overlap: Option<f32>,
    org_overlap: Option<f32>,
    location_overlap: Option<f32>,
    event_overlap: Option<f32>,
    product_overlap: Option<f32>,
    temporal_proximity: Option<f32>,
    match_formula: String,
    reason_for_failure: Option<String>,
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

/// Analyze the matching between two specific articles to understand why they
/// match or don't match. This is a diagnostic endpoint for tuning the matching algorithm.
async fn analyze_article_match(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    _headers: HeaderMap,
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<ArticleMatchAnalysisRequest>,
) -> Result<Json<ArticleMatchAnalysisResponse>, StatusCode> {
    // Validate the JWT token
    let token = auth_header.token();
    if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!(
        "Analyzing match between articles {} and {}",
        payload.source_article_id, payload.target_article_id
    );

    // Get embeddings for both articles from Qdrant
    let source_article_id = payload.source_article_id;
    let target_article_id = payload.target_article_id;

    // Check if this is a self-match (comparing an article to itself)
    let is_self_match = source_article_id == target_article_id;

    // Initialize response defaults
    let mut response = ArticleMatchAnalysisResponse {
        source_article_id,
        target_article_id,
        is_self_match,
        vector_similarity: 0.0,
        entity_similarity: None,
        combined_score: 0.0,
        threshold: 0.75, // Standard threshold
        match_status: false,
        source_entity_count: 0,
        target_entity_count: 0,
        shared_entity_count: 0,
        shared_primary_entity_count: 0,
        person_overlap: None,
        org_overlap: None,
        location_overlap: None,
        event_overlap: None,
        product_overlap: None,
        temporal_proximity: None,
        match_formula: "60% vector similarity + 40% entity similarity".to_string(),
        reason_for_failure: None,
    };

    // For self-matches, add a note about not displaying in UI
    if is_self_match {
        info!(
            "Self-match detected for article {}, this would be filtered in related articles",
            source_article_id
        );
        response.reason_for_failure =
            Some("Self-matches are filtered from related articles in the UI".to_string());
    }

    // Step 1: Get both articles' entity data
    let source_entities = match get_article_entities(source_article_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            response.reason_for_failure =
                Some("Source article has no extracted entities".to_string());
            return Ok(Json(response));
        }
        Err(e) => {
            warn!(
                "Failed to get entities for source article {}: {}",
                source_article_id, e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let target_entities = match get_article_entities(target_article_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            response.reason_for_failure =
                Some("Target article has no extracted entities".to_string());
            return Ok(Json(response));
        }
        Err(e) => {
            warn!(
                "Failed to get entities for target article {}: {}",
                target_article_id, e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Step 2: Calculate vector similarity
    let vector_similarity =
        match crate::vector::storage::get_article_vector_from_qdrant(source_article_id).await {
            Ok(source_vector) => {
                match crate::vector::storage::get_article_vector_from_qdrant(target_article_id)
                    .await
                {
                    Ok(target_vector) => {
                        match crate::vector::similarity::calculate_direct_similarity(
                            &source_vector,
                            &target_vector,
                        ) {
                            Ok(similarity) => similarity,
                            Err(e) => {
                                warn!("Failed to calculate direct vector similarity: {}", e);
                                response.reason_for_failure =
                                    Some("Failed to calculate vector similarity".to_string());
                                return Ok(Json(response));
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to retrieve target article vector: {}", e);
                        response.reason_for_failure =
                            Some("Failed to retrieve target article vector".to_string());
                        return Ok(Json(response));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to retrieve source article vector: {}", e);
                response.reason_for_failure =
                    Some("Failed to retrieve source article vector".to_string());
                return Ok(Json(response));
            }
        };

    response.vector_similarity = vector_similarity;

    // Step 3: Calculate entity similarity
    let db = Database::instance().await;
    let (source_pub_date, source_event_date) =
        match db.get_article_details_with_dates(source_article_id).await {
            Ok(dates) => dates,
            Err(e) => {
                warn!("Failed to get source article dates: {}", e);
                (None, None)
            }
        };

    let (target_pub_date, _) = match db.get_article_details_with_dates(target_article_id).await {
        Ok(dates) => dates,
        Err(e) => {
            warn!("Failed to get target article dates: {}", e);
            (None, None)
        }
    };

    // Calculate entity similarity
    let entity_sim = calculate_entity_similarity(
        &source_entities,
        &target_entities,
        source_event_date.as_deref().or(source_pub_date.as_deref()),
        target_pub_date.as_deref(),
    );

    // Update response with entity details
    response.entity_similarity = Some(entity_sim.combined_score);
    response.source_entity_count = source_entities.entities.len();
    response.target_entity_count = target_entities.entities.len();
    response.shared_entity_count = entity_sim.entity_overlap_count;
    response.shared_primary_entity_count = entity_sim.primary_overlap_count;
    response.person_overlap = Some(entity_sim.person_overlap);
    response.org_overlap = Some(entity_sim.organization_overlap);
    response.location_overlap = Some(entity_sim.location_overlap);
    response.event_overlap = Some(entity_sim.event_overlap);
    response.product_overlap = Some(entity_sim.product_overlap);
    response.temporal_proximity = Some(entity_sim.temporal_proximity);

    // Calculate combined score (60% vector + 40% entity)
    response.combined_score = 0.6 * vector_similarity + 0.4 * entity_sim.combined_score;

    // Determine match status
    response.match_status = response.combined_score >= response.threshold;

    // Add detailed reason if not matching
    if !response.match_status {
        let missing_score = response.threshold - response.combined_score;

        if entity_sim.entity_overlap_count == 0 {
            response.reason_for_failure = Some(format!(
                "No shared entities. Articles with no entity overlap cannot match regardless of vector similarity."
            ));
        } else if vector_similarity < 0.5 {
            response.reason_for_failure = Some(format!(
                "Low vector similarity ({:.2}). Articles need at least {:.2} more points to reach threshold.",
                vector_similarity, missing_score
            ));
        } else if entity_sim.combined_score < 0.3 {
            response.reason_for_failure = Some(format!(
                "Weak entity similarity ({:.2}). Despite sharing {} entities, the importance levels or entity types don't align well.",
                entity_sim.combined_score, entity_sim.entity_overlap_count
            ));
        } else {
            response.reason_for_failure = Some(format!(
                "Combined score ({:.2}) below threshold ({:.2}). Needs {:.2} more points to match.",
                response.combined_score, response.threshold, missing_score
            ));
        }
    }

    info!("Match analysis result: articles {} and {} - match={}, score={:.2}, vector={:.2}, entity={:.2}, shared_entities={}",
          source_article_id, target_article_id, response.match_status, response.combined_score,
          vector_similarity, entity_sim.combined_score, entity_sim.entity_overlap_count);

    Ok(Json(response))
}

/// Sync clusters endpoint handler
async fn sync_clusters(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<SyncClustersRequest>,
) -> Result<Json<SyncClustersResponse>, StatusCode> {
    // Validate the JWT token
    let token = auth_header.token();
    if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let client_ip = get_client_ip(&headers, &addr);
    info!("app::api sync_clusters request from IP {}", client_ip);

    let db = Database::instance().await;

    // Create maps of known cluster IDs and versions
    let mut known_clusters = HashMap::new();
    for info in payload.known_clusters {
        known_clusters.insert(info.id, info.version);
    }

    // Get all active clusters
    let active_clusters = crate::db::cluster::get_active_clusters(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut updated_clusters = Vec::new();

    // Add clusters that are new or have updated versions
    for cluster in active_clusters {
        let id = cluster.id;
        let version = cluster.summary_version;

        let known_version = known_clusters.get(&id).copied().unwrap_or(-1);

        // If client doesn't know this cluster or has an outdated version
        if known_version < version {
            // Get articles for this cluster using the proper db function
            let articles = crate::db::cluster::get_cluster_articles_brief(&db, id, 5)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let cluster_data = ClusterData {
                id,
                version,
                creation_date: cluster.creation_date,
                summary: cluster.summary,
                article_count: cluster.article_count,
                importance_score: cluster.importance_score,
                has_timeline: cluster.has_timeline,
                articles,
            };

            updated_clusters.push(cluster_data);
        }
    }

    // Get merged clusters
    let merged_clusters_data = crate::clustering::get_merged_clusters(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut merged_clusters = Vec::new();
    let mut deleted_clusters = Vec::new();

    for (original_id, merged_into_id) in merged_clusters_data {
        // Only include if client knew about the original cluster
        if known_clusters.contains_key(&original_id) {
            merged_clusters.push(ClusterMergeInfo {
                original_id,
                merged_into_id,
            });
        }
    }

    // Deleted clusters would be any in the client's known list that are
    // neither active nor merged into something else
    for &id in known_clusters.keys() {
        let exists = crate::db::cluster::does_cluster_exist(&db, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if !exists {
            deleted_clusters.push(id);
        }
    }

    let response = SyncClustersResponse {
        updated_clusters,
        deleted_clusters,
        merged_clusters,
    };

    Ok(Json(response))
}

/// Main application loop, setting up and running the Axum-based API server.
pub async fn app_api_loop() -> Result<()> {
    let app = Router::new()
        .route("/status", post(status_check))
        .route("/authenticate", post(authenticate))
        .route("/subscriptions", post(get_subscriptions))
        .route("/subscribe", post(subscribe_to_topic))
        .route("/unsubscribe", post(unsubscribe_from_topic))
        .route("/articles/sync", post(sync_seen_articles))
        .route("/articles/analyze-match", post(analyze_article_match))
        .route("/clusters/sync", post(sync_clusters));

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
    headers: HeaderMap,
    Json(payload): Json<AuthRequest>,
) -> Json<AuthResponse> {
    let client_ip = get_client_ip(&headers, &addr);
    info!(
        "Authenticating device_id: {} from IP: {}",
        payload.device_id, client_ip,
    );

    // Record IP address.
    let db = Database::instance().await;
    if let Err(e) = db.log_ip_address(&payload.device_id, &client_ip).await {
        warn!("Failed to log IP address: {:?}", e);
    }

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
    headers: HeaderMap,
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let client_ip = get_client_ip(&headers, &addr);
    info!(
        "app::api subscribe_to_topic request from IP {} for topic: {}",
        client_ip, payload.topic
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

    // Record IP address.
    let db = Database::instance().await;
    if let Err(e) = db.log_ip_address(&device_id, &client_ip).await {
        warn!("Failed to log IP address: {:?}", e);
    }

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
    headers: HeaderMap,
    auth_header: TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<TopicRequest>,
) -> Result<StatusCode, StatusCode> {
    let client_ip = get_client_ip(&headers, &addr);
    info!(
        "app::api unsusbcribe_from_topic request from IP {} for topic: {}",
        client_ip, payload.topic
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

    // Record IP address.
    let db = Database::instance().await;
    if let Err(e) = db.log_ip_address(&device_id, &client_ip).await {
        warn!("Failed to log IP address: {:?}", e);
    }

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
    headers: HeaderMap,
    auth_header: Option<TypedHeader<Authorization<Bearer>>>,
) -> Result<&'static str, StatusCode> {
    let client_ip = get_client_ip(&headers, &addr);
    info!("app::api status_check request from IP {} ", client_ip,);
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

    let db = Database::instance().await;
    if let Err(e) = db.log_ip_address(&device_id, &client_ip).await {
        warn!("Failed to log IP address: {:?}", e);
    }

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

// [Nothing here - remove this duplicate function]

async fn get_subscriptions(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    auth_header: TypedHeader<Authorization<Bearer>>,
) -> Result<Json<SubscriptionsResponse>, StatusCode> {
    let client_ip = get_client_ip(&headers, &addr);
    info!("app::api get_subscriptions request from IP {}", client_ip);

    let token = auth_header.token();
    let claims = decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256))
        .map_err(|e| {
            warn!("app::api get_subscriptions JWT validation failed: {:#?}", e);
            StatusCode::UNAUTHORIZED
        })?;
    let device_id = claims.claims.sub;

    info!(
        "app::api get_subscriptions validated JWT for device_id: {}",
        device_id
    );

    // Record IP address
    let db = Database::instance().await;
    if let Err(e) = db.log_ip_address(&device_id, &client_ip).await {
        warn!("Failed to log IP address: {:?}", e);
    }

    // Get subscriptions from database
    match db.get_device_subscriptions(&device_id).await {
        Ok(subscriptions) => {
            info!(
                "app::api get_subscriptions successfully retrieved subscriptions for device_id: {}",
                device_id
            );
            Ok(Json(SubscriptionsResponse { subscriptions }))
        }
        Err(e) => {
            warn!("app::api get_subscriptions unexpected error: {:#?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
