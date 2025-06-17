# 05 - Code Changes & Integration

**Effort**: L (few days)

## Database Configuration

### Create src/db/config.rs
```rust
use std::env;

#[derive(Debug, Clone)]
pub enum DatabaseType {
    Sqlite(String),  // path
    Postgres(String), // url
}

impl DatabaseType {
    pub fn from_env() -> Self {
        match env::var("DATABASE_TYPE").unwrap_or_default().as_str() {
            "postgres" => {
                let url = env::var("DATABASE_URL")
                    .expect("DATABASE_URL required for PostgreSQL");
                Self::Postgres(url)
            }
            _ => {
                let path = env::var("SQLITE_PATH")
                    .unwrap_or_else(|_| "argus.db".to_string());
                Self::Sqlite(path)
            }
        }
    }
}
```

## Enhanced Database Core

### Update src/db/core.rs  
```rust
use sqlx::{Pool, Postgres, Sqlite, Row};
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqlitePoolOptions, SqliteConnectOptions};
use std::str::FromStr;
use tokio::time::Duration;
use super::config::DatabaseType;

#[derive(Clone)]
pub enum DatabasePool {
    Sqlite(Pool<Sqlite>),
    Postgres(Pool<Postgres>),
}

#[derive(Clone)]
pub struct Database {
    pool: DatabasePool,
}

impl Database {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let config = DatabaseType::from_env();
        
        let pool = match config {
            DatabaseType::Sqlite(path) => {
                let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path))?
                    .create_if_missing(true)
                    .busy_timeout(Duration::from_secs(10));
                
                let pool = SqlitePoolOptions::new()
                    .max_connections(1) // SQLite limitation
                    .connect_with(options)
                    .await?;
                
                DatabasePool::Sqlite(pool)
            }
            
            DatabaseType::Postgres(url) => {
                let pool = PgPoolOptions::new()
                    .max_connections(20)
                    .acquire_timeout(Duration::from_secs(30))
                    .connect(&url)
                    .await?;
                
                DatabasePool::Postgres(pool)
            }
        };
        
        Ok(Database { pool })
    }
    
    // Configuration management
    pub async fn set_config(&self, category: &str, name: &str, value: &str) -> Result<(), sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO configurations (category, name, value, updated_at)
                     VALUES ($1, $2, $3, NOW())
                     ON CONFLICT (category, name) 
                     DO UPDATE SET value = $3, updated_at = NOW()"
                )
                .bind(category)
                .bind(name) 
                .bind(value)
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT OR REPLACE INTO configurations (category, name, value, updated_at)
                     VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)"
                )
                .bind(category)
                .bind(name)
                .bind(value)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }
    
    pub async fn get_config(&self, category: &str, name: &str) -> Result<Option<String>, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query_scalar(
                    "SELECT value FROM configurations 
                     WHERE category = $1 AND name = $2 AND enabled = true"
                )
                .bind(category)
                .bind(name)
                .fetch_optional(pool)
                .await
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query_scalar(
                    "SELECT value FROM configurations 
                     WHERE category = ?1 AND name = ?2 AND enabled = true"
                )
                .bind(category)
                .bind(name)
                .fetch_optional(pool)
                .await
            }
        }
    }
    
    pub async fn get_configs_by_category(&self, category: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT name, value FROM configurations 
                     WHERE category = $1 AND enabled = true ORDER BY name"
                )
                .bind(category) 
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (row.get("name"), row.get("value"))
                }).collect())
            }
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT name, value FROM configurations 
                     WHERE category = ?1 AND enabled = true ORDER BY name"
                )
                .bind(category)
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (row.get("name"), row.get("value"))
                }).collect())
            }
        }
    }
}
```

## Configuration Manager

### Create src/config/mod.rs
```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use crate::db::Database;

pub struct ConfigManager {
    cache: Arc<RwLock<HashMap<String, String>>>,
    db: Arc<Database>,
}

impl ConfigManager {
    pub async fn new() -> Result<Self> {
        let db = Arc::new(Database::new().await?);
        let cache = Arc::new(RwLock::new(HashMap::new()));
        
        let manager = ConfigManager { cache, db };
        manager.refresh_cache().await?;
        
        Ok(manager)
    }
    
    async fn refresh_cache(&self) -> Result<()> {
        let topics = self.db.get_configs_by_category("topics").await?;
        let feeds = self.db.get_configs_by_category("rss_feeds").await?;
        let system = self.db.get_configs_by_category("system").await?;
        
        let mut cache = self.cache.write().await;
        cache.clear();
        
        for (name, value) in topics {
            cache.insert(format!("topics:{}", name), value);
        }
        for (name, value) in feeds {
            cache.insert(format!("rss_feeds:{}", name), value);
        }
        for (name, value) in system {
            cache.insert(format!("system:{}", name), value);
        }
        
        Ok(())
    }
    
    pub async fn get_topics(&self) -> Vec<(String, String)> {
        let cache = self.cache.read().await;
        cache.iter()
            .filter(|(k, _)| k.starts_with("topics:"))
            .map(|(k, v)| (k.strip_prefix("topics:").unwrap().to_string(), v.clone()))
            .collect()
    }
    
    pub async fn get_rss_feeds(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache.iter()
            .filter(|(k, _)| k.starts_with("rss_feeds:"))
            .map(|(_, v)| v.clone())
            .collect()
    }
    
    pub async fn get_system_setting(&self, key: &str) -> Option<String> {
        let cache_key = format!("system:{}", key);
        let cache = self.cache.read().await;
        cache.get(&cache_key).cloned()
    }
    
    pub async fn add_topic(&self, name: &str, prompt: &str) -> Result<()> {
        self.db.set_config("topics", name, prompt).await?;
        self.refresh_cache().await?;
        Ok(())
    }
    
    pub async fn add_rss_feed(&self, name: &str, url: &str) -> Result<()> {
        self.db.set_config("rss_feeds", name, url).await?;
        self.refresh_cache().await?;
        Ok(())
    }
}
```

## Environment to Database Migration

### Create src/bin/migrate_env_to_db.rs
```rust
use anyhow::Result;
use argus::db::Database;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Database::new().await?;
    
    // Migrate topics from TOPICS env var
    if let Ok(topics_str) = env::var("TOPICS") {
        for line in topics_str.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            
            for topic_pair in line.split(';') {
                let topic_pair = topic_pair.trim();
                if topic_pair.is_empty() { continue; }
                
                let parts: Vec<&str> = topic_pair.split(':').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim();
                    let prompt = parts[1].trim();
                    
                    if !name.is_empty() && !prompt.is_empty() {
                        db.set_config("topics", name, prompt).await?;
                        println!("Migrated topic: {} -> {}", name, prompt);
                    }
                }
            }
        }
    }
    
    // Migrate RSS feeds from URLS env var
    if let Ok(urls_str) = env::var("URLS") {
        for (i, url) in urls_str.split(';').enumerate() {
            let url = url.trim();
            if !url.is_empty() {
                let name = format!("feed_{}", i + 1);
                db.set_config("rss_feeds", &name, url).await?;
                println!("Migrated RSS feed: {} -> {}", name, url);
            }
        }
    }
    
    // Migrate system settings
    let system_vars = [
        ("slack_token", "SLACK_TOKEN"),
        ("slack_channel", "SLACK_CHANNEL"), 
        ("places_json_path", "PLACES_JSON_PATH"),
        ("rust_log", "RUST_LOG"),
    ];
    
    for (key, env_var) in system_vars {
        if let Ok(value) = env::var(env_var) {
            db.set_config("system", key, &value).await?;
            println!("Migrated system setting: {} -> {}", key, 
                     if key == "slack_token" { "[REDACTED]" } else { &value });
        }
    }
    
    println!("✅ Environment configuration migrated to database");
    Ok(())
}
```

## Update Main Application

### Update src/main.rs
```rust
use argus::config::ConfigManager;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    
    // Initialize configuration manager
    let config_manager = ConfigManager::new().await?;
    
    // Get configurations from database instead of environment
    let topics = config_manager.get_topics().await;
    let rss_feeds = config_manager.get_rss_feeds().await;
    
    // Get system settings
    let slack_token = config_manager.get_system_setting("slack_token").await
        .expect("SLACK_TOKEN not configured in database");
    let slack_channel = config_manager.get_system_setting("slack_channel").await
        .expect("SLACK_CHANNEL not configured in database");
    
    println!("Loaded configuration: {} topics, {} RSS feeds",
             topics.len(), rss_feeds.len());
    
    // Start application with database configuration
    start_application(topics, rss_feeds, slack_token, slack_channel).await?;
    
    Ok(())
}

async fn start_application(
    topics: Vec<(String, String)>,
    rss_feeds: Vec<String>,
    slack_token: String,
    slack_channel: String,
) -> Result<()> {
    // Implementation uses database configuration instead of environment
    // ... (existing application startup logic)
    Ok(())
}
```

## API Configuration Endpoints

### Add to src/app/api.rs
```rust
use argus::config::ConfigManager;
use axum::{extract::Path, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CreateTopicRequest {
    name: String,
    prompt: String,
}

#[derive(Deserialize)]
struct CreateFeedRequest {
    name: String,
    url: String,
}

#[derive(Serialize)]
struct TopicResponse {
    name: String,
    prompt: String,
}

#[derive(Serialize)]
struct FeedResponse {
    name: String,
    url: String,
}

// Get all topics (admin only)
async fn get_admin_topics() -> Result<Json<Vec<TopicResponse>>, StatusCode> {
    let config_manager = ConfigManager::new().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let topics = config_manager.get_topics().await;
    let response: Vec<TopicResponse> = topics.into_iter()
        .map(|(name, prompt)| TopicResponse { name, prompt })
        .collect();
    
    Ok(Json(response))
}

// Add new topic (admin only)
async fn add_topic(Json(payload): Json<CreateTopicRequest>) -> Result<StatusCode, StatusCode> {
    let config_manager = ConfigManager::new().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    config_manager.add_topic(&payload.name, &payload.prompt).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::CREATED)
}

// Delete topic (admin only)
async fn delete_topic(Path(topic_name): Path<String>) -> Result<StatusCode, StatusCode> {
    let config_manager = ConfigManager::new().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    config_manager.db.set_config("topics", &topic_name, "").await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::OK)
}

// Add to router
pub fn admin_routes() -> Router {
    Router::new()
        .route("/topics", get(get_admin_topics).post(add_topic))
        .route("/topics/:name", delete(delete_topic))
        .route("/feeds", get(get_admin_feeds).post(add_feed))
        .route("/feeds/:name", delete(delete_feed))
}
```

## Testing Binary

### Create src/bin/test_config_migration.rs
```rust
use anyhow::Result;
use argus::{config::ConfigManager, db::Database};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Testing configuration migration...");
    
    // Test database configuration
    let db = Database::new().await?;
    
    // Test setting configuration
    db.set_config("test", "key1", "value1").await?;
    println!("✅ Set test configuration");
    
    // Test getting configuration
    let value = db.get_config("test", "key1").await?;
    assert_eq!(value, Some("value1".to_string()));
    println!("✅ Retrieved test configuration");
    
    // Test configuration manager
    let config_manager = ConfigManager::new().await?;
    
    // Test adding topic
    config_manager.add_topic("test_topic", "Test topic prompt").await?;
    println!("✅ Added test topic");
    
    // Test getting topics
    let topics = config_manager.get_topics().await;
    println!("✅ Retrieved {} topics", topics.len());
    
    // Test system settings
    if let Some(setting) = config_manager.get_system_setting("rust_log").await {
        println!("✅ Retrieved system setting: {}", setting);
    }
    
    println!("🎉 Configuration migration test completed successfully!");
    Ok(())
}
```

## Module Updates

### Update src/lib.rs
```rust
pub mod config;
pub mod db;
// ... existing modules
```

### Update src/db/mod.rs
```rust
pub mod config;
pub mod core;
// ... existing modules

pub use core::Database;
pub use config::DatabaseType;
```

## Integration Steps
1. **Create new modules**: Add database config and configuration manager
2. **Build migration tools**: `cargo build --bin migrate_env_to_db --bin test_config_migration`
3. **Run environment migration**: `cargo run --bin migrate_env_to_db`
4. **Test integration**: `cargo run --bin test_config_migration`
5. **Update main application**: Modify startup to use ConfigManager
6. **Add API endpoints**: Include admin configuration routes
7. **Validate functionality**: Ensure all existing features work with database config
