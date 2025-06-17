# Argus SQLite to PostgreSQL Migration Plan with Database Configuration

## Overview
This comprehensive plan migrates Argus from SQLite to PostgreSQL while simultaneously moving ALL configuration from environment variables to database storage. This migration resolves database locking issues, improves performance, and enables runtime configuration management for a scalable public service.

**Problem Statement**: 
- SQLite "database is locked" errors under high concurrent load
- Environment variable configuration requires restarts for changes
- Manual coordination needed between backend config and frontend validation

**Solution**: 
- Migrate to PostgreSQL for better concurrent handling
- Move all configuration (topics, RSS feeds, LLM servers, system settings) to database
- Enable runtime configuration management through admin APIs

**Estimated Timeline**: 8 days
**Risk Level**: Medium (well-isolated database layer + configuration abstraction)
**Expected Benefits**: 
- Elimination of locking errors
- 20-50% performance improvement  
- Runtime topic/feed/worker management
- Scalable configuration for public service deployment

---

## Pre-Migration Checklist

### Environment Setup
1. **PostgreSQL Installation**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install postgresql postgresql-contrib
   # macOS
   brew install postgresql
   # Start service
   sudo systemctl start postgresql
   ```

2. **Database Creation**
   ```sql
   CREATE DATABASE argus_prod;
   CREATE DATABASE argus_dev;
   CREATE USER argus_user WITH PASSWORD 'your_secure_password';
   GRANT ALL PRIVILEGES ON DATABASE argus_prod TO argus_user;
   GRANT ALL PRIVILEGES ON DATABASE argus_dev TO argus_user;
   ```

3. **Backup Current SQLite Database**
   ```bash
   cp argus.db argus.db.backup.$(date +%Y%m%d_%H%M%S)
   ```

---

## Phase 1: Code Preparation (Days 1-2)

### 1.1 Update Dependencies

**Cargo.toml changes:**
```toml
# Replace this line:
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls", "macros"] }

# With this (keep both during transition):
sqlx = { version = "0.8", features = ["sqlite", "postgres", "runtime-tokio-rustls", "macros", "uuid", "chrono", "json"] }
serde_json = "1.0"
```

### 1.2 Enhanced Environment Configuration

**Update env.template:**
```bash
# Database Configuration
DATABASE_TYPE=postgres  # or sqlite for fallback
DATABASE_URL=postgresql://argus_user:your_secure_password@localhost/argus_prod
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_USER=argus_user
POSTGRES_PASSWORD=your_secure_password
POSTGRES_DB=argus_prod

# Keep existing for fallback during migration
DATABASE_PATH=argus.db

# CONFIGURATION MIGRATION NOTE:
# After migration, these environment variables will be stored in database
# and can be removed from .env file

# Topics Configuration (will move to database)
export TOPICS="
  Tuscany:Tuscany, the famous region in Italy;
  Space:Space and Space Exploration;
  Bitcoins:Bitcoins, the cryptocurrency;
  EVs:Electric Cars;
  Apple:New Apple products, like new versions of iPhone, iPad and MacBooks;
  LLMs:The Llama large language model;"

# RSS Feeds Configuration (will move to database)
export URLS="
  http://rss.slashdot.org/Slashdot/slashdot;
  https://9to5mac.com/rss;
  https://hnrss.org/frontpage;"

# Worker Configurations (will move to database)
export DECISION_OLLAMA_CONFIGS="localhost|11434|qwen2.5:7b/no_think"
export ANALYSIS_OLLAMA_CONFIGS="localhost|11434|llama3.1:70b||localhost|11434|qwen2.5:7b/no_think"

# System Configuration (will move to database)
export SLACK_TOKEN="xoxb-XXXXXXXXXXXX"
export SLACK_CHANNEL="CXXXXXXXXX"
export PLACES_JSON_PATH="places.json"
export RUST_LOG="info"
```

### 1.3 Database Abstraction Layer

**Create src/db/config.rs:**
```rust
use serde::Deserialize;
use std::env;

#[derive(Debug, Clone)]
pub enum DatabaseConfig {
    Sqlite { path: String },
    Postgres { url: String },
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        let db_type = env::var("DATABASE_TYPE").unwrap_or_else(|_| "sqlite".to_string());
        
        match db_type.as_str() {
            "postgres" => {
                let url = env::var("DATABASE_URL")
                    .expect("DATABASE_URL must be set when using PostgreSQL");
                Self::Postgres { url }
            }
            "sqlite" | _ => {
                let path = env::var("DATABASE_PATH")
                    .unwrap_or_else(|_| "argus.db".to_string());
                Self::Sqlite { path }
            }
        }
    }
}
```

### 1.4 Configuration Manager Structure

**Create src/config/mod.rs:**
```rust
pub mod manager;
pub mod types;

pub use manager::ConfigManager;
pub use types::*;
```

**Create src/config/types.rs:**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicConfig {
    pub name: String,
    pub prompt: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFeedConfig {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionWorkerConfig {
    pub name: String,
    pub worker_type: String, // "ollama" or "openai"
    pub host: Option<String>,
    pub port: Option<u16>,
    pub api_key: Option<String>,
    pub model: String,
    pub no_think: bool,
    pub parameters: WorkerParameters,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisWorkerConfig {
    pub name: String,
    pub worker_type: String, // "ollama" or "openai"
    pub host: Option<String>,
    pub port: Option<u16>,
    pub api_key: Option<String>,
    pub primary_model: String,
    pub fallback_model: Option<String>,
    pub no_think_primary: bool,
    pub no_think_fallback: Option<bool>,
    pub parameters_primary: WorkerParameters,
    pub parameters_fallback: Option<WorkerParameters>,
    pub fallback_config: Option<FallbackConfig>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerParameters {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub min_p: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub idle_timeout_minutes: u32,
    pub fallback_duration_minutes: u32,
}

impl Default for WorkerParameters {
    fn default() -> Self {
        Self {
            temperature: None,
            top_p: None,
            top_k: None,
            min_p: None,
        }
    }
}
```

---

## Phase 2: Enhanced Schema Migration (Day 3)

### 2.1 Complete PostgreSQL Schema with Configuration Tables

**Create migrations/postgresql_schema.sql:**
```sql
-- ===== CONFIGURATION TABLES =====

-- Core configuration storage
CREATE TABLE IF NOT EXISTS configurations (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    value TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(category, name)
);

-- Configuration change audit trail
CREATE TABLE IF NOT EXISTS configuration_changes (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    old_value TEXT,
    new_value TEXT,
    changed_by VARCHAR(100),
    changed_at TIMESTAMPTZ DEFAULT NOW(),
    change_reason TEXT
);

-- Indexes for configuration tables
CREATE INDEX IF NOT EXISTS idx_configurations_category ON configurations(category);
CREATE INDEX IF NOT EXISTS idx_configurations_enabled ON configurations(category, enabled);
CREATE INDEX IF NOT EXISTS idx_configuration_changes_category ON configuration_changes(category, name);

-- ===== EXISTING ARGUS TABLES (PostgreSQL Compatible) =====

-- Articles table
CREATE TABLE IF NOT EXISTS articles (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ,
    event_date TIMESTAMPTZ,
    title TEXT,
    source TEXT,
    is_relevant BOOLEAN NOT NULL,
    category TEXT,
    tiny_summary TEXT,
    analysis JSONB,  -- JSONB for better performance
    json_data JSONB,
    quality REAL,
    hash TEXT,
    title_domain_hash TEXT,
    r2_url TEXT,
    cluster_id BIGINT
);

-- Article indexes
CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
CREATE INDEX IF NOT EXISTS idx_hash ON articles (hash);
CREATE INDEX IF NOT EXISTS idx_title_domain_hash ON articles (title_domain_hash);
CREATE INDEX IF NOT EXISTS idx_r2_url ON articles (r2_url);
CREATE INDEX IF NOT EXISTS idx_seen_at_r2_url ON articles (seen_at, r2_url);
CREATE INDEX IF NOT EXISTS idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url);
CREATE INDEX IF NOT EXISTS idx_articles_event_date ON articles (event_date);
CREATE INDEX IF NOT EXISTS idx_articles_pub_date ON articles (pub_date);
CREATE INDEX IF NOT EXISTS idx_articles_cluster_id ON articles (cluster_id);

-- JSONB indexes for analysis field
CREATE INDEX IF NOT EXISTS idx_articles_analysis_quality ON articles USING GIN ((analysis->'quality'));

-- Entity tables
CREATE TABLE IF NOT EXISTS entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    parent_id BIGINT,
    UNIQUE(normalized_name, type),
    FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_normalized_name ON entities (normalized_name);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities (type);
CREATE INDEX IF NOT EXISTS idx_entities_parent_id ON entities (parent_id);

-- Article-Entity relationships
CREATE TABLE IF NOT EXISTS article_entities (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    entity_id BIGINT NOT NULL,
    importance TEXT NOT NULL,
    context TEXT,
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
    UNIQUE(article_id, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_article_entities_article_id ON article_entities (article_id);
CREATE INDEX IF NOT EXISTS idx_article_entities_entity_id ON article_entities (entity_id);
CREATE INDEX IF NOT EXISTS idx_article_entities_importance ON article_entities (importance);

-- Entity aliases
CREATE TABLE IF NOT EXISTS entity_aliases (
    id BIGSERIAL PRIMARY KEY,
    entity_id BIGINT NOT NULL,
    alias TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
    UNIQUE(normalized_alias, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_entity_aliases_normalized_alias ON entity_aliases (normalized_alias);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_entity_id ON entity_aliases (entity_id);

-- Article clusters
CREATE TABLE IF NOT EXISTS article_clusters (
    id BIGSERIAL PRIMARY KEY,
    name TEXT,
    creation_date TIMESTAMPTZ NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL,
    primary_entity_ids JSONB NOT NULL DEFAULT '[]',
    article_count INTEGER NOT NULL DEFAULT 0,
    needs_summary_update INTEGER NOT NULL DEFAULT 1,
    summary TEXT,
    summary_version INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    importance_score REAL NOT NULL DEFAULT 0.0,
    has_timeline INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_article_clusters_last_updated ON article_clusters (last_updated);
CREATE INDEX IF NOT EXISTS idx_article_clusters_status ON article_clusters (status);
CREATE INDEX IF NOT EXISTS idx_article_clusters_importance ON article_clusters (importance_score);

-- Article cluster mappings
CREATE TABLE IF NOT EXISTS article_cluster_mappings (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    cluster_id BIGINT NOT NULL,
    similarity_score REAL NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
    UNIQUE(article_id, cluster_id)
);

CREATE INDEX IF NOT EXISTS idx_article_cluster_mappings_article_id ON article_cluster_mappings (article_id);
CREATE INDEX IF NOT EXISTS idx_article_cluster_mappings_cluster_id ON article_cluster_mappings (cluster_id);

-- Cluster merge history
CREATE TABLE IF NOT EXISTS cluster_merge_history (
    id BIGSERIAL PRIMARY KEY,
    original_cluster_id BIGINT NOT NULL,
    merged_into_cluster_id BIGINT NOT NULL,
    merge_reason TEXT,
    merged_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cluster_merge_history_original ON cluster_merge_history (original_cluster_id);
CREATE INDEX IF NOT EXISTS idx_cluster_merge_history_merged_into ON cluster_merge_history (merged_into_cluster_id);

-- Queue tables
CREATE TABLE IF NOT EXISTS rss_queue (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    title TEXT,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
CREATE UNIQUE INDEX IF NOT EXISTS idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url);

CREATE TABLE IF NOT EXISTS matched_topics_queue (
    id BIGSERIAL PRIMARY KEY,
    article_text TEXT NOT NULL,
    article_html TEXT NOT NULL,
    article_url TEXT NOT NULL,
    article_title TEXT NOT NULL,
    article_hash TEXT NOT NULL,
    title_domain_hash TEXT NOT NULL,
    topic_matched TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_matched_topics_article_url ON matched_topics_queue (article_url);

CREATE TABLE IF NOT EXISTS life_safety_queue (
    id BIGSERIAL PRIMARY KEY,
    article_text TEXT NOT NULL,
    article_html TEXT NOT NULL,
    article_url TEXT NOT NULL,
    article_title TEXT NOT NULL,
    article_hash TEXT NOT NULL,
    title_domain_hash TEXT NOT NULL,
    threat_type TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_life_safety_article_url ON life_safety_queue (article_url);

-- Device management tables
CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE,
    device_type TEXT,
    app_version TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_seen TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_devices_device_id ON devices (device_id);

CREATE TABLE IF NOT EXISTS device_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL,
    topic TEXT NOT NULL,
    priority TEXT,
    FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
    UNIQUE(device_id, topic)
);

CREATE INDEX IF NOT EXISTS idx_topic_device_id ON device_subscriptions (topic, device_id);
CREATE INDEX IF NOT EXISTS idx_device_subscriptions_device_id_topic ON device_subscriptions (device_id, topic);

-- IP logging table
CREATE TABLE IF NOT EXISTS ip_logs (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL,
    ip_address TEXT NOT NULL,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ip_logs_device_id ON ip_logs (device_id);
CREATE INDEX IF NOT EXISTS idx_ip_logs_timestamp ON ip_logs (timestamp);

-- Endpoint monitoring tables
CREATE TABLE IF NOT EXISTS endpoint_timeout_events (
    id BIGSERIAL PRIMARY KEY,
    endpoint_name TEXT NOT NULL,
    timeout_duration INTEGER NOT NULL,
    error_message TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_timeout_events_endpoint ON endpoint_timeout_events (endpoint_name);
CREATE INDEX IF NOT EXISTS idx_timeout_events_timestamp ON endpoint_timeout_events (timestamp);

CREATE TABLE IF NOT EXISTS endpoint_alerts (
    id BIGSERIAL PRIMARY KEY,
    endpoint_name TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    is_resolved BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_endpoint_alerts_endpoint ON endpoint_alerts (endpoint_name);
CREATE INDEX IF NOT EXISTS idx_endpoint_alerts_resolved ON endpoint_alerts (is_resolved);
```

---

## Phase 3: Data and Configuration Migration (Day 4)

### 3.1 Environment to Database Migration Binary

**Create src/bin/migrate_env_to_db.rs:**
```rust
use anyhow::Result;
use argus::db::Database;
use serde_json::json;
use std::env;
use tracing::{info, warn};

async fn migrate_topics(db: &Database) -> Result<()> {
    info!("Migrating topics from TOPICS environment variable...");
    
    let topics_str = env::var("TOPICS").unwrap_or_default();
    let mut count = 0;
    
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
                    db.set_configuration("topics", name, prompt).await?;
                    count += 1;
                    info!("Migrated topic: {} -> {}", name, prompt);
                }
            }
        }
    }
    
    info!("Successfully migrated {} topics", count);
    Ok(())
}

async fn migrate_rss_feeds(db: &Database) -> Result<()> {
    info!("Migrating RSS feeds from URLS environment variable...");
    
    let urls_str = env::var("URLS").unwrap_or_default();
    let mut count = 0;
    
    for line in urls_str.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        
        for url in line.split(';') {
            let url = url.trim();
            if !url.is_empty() {
                // Extract domain name for the configuration name
                let name = if let Ok(parsed_url) = url::Url::parse(url) {
                    parsed_url.host_str()
                        .unwrap_or("unknown")
                        .replace("www.", "")
                        .replace('.', "_")
                } else {
                    format!("feed_{}", count + 1)
                };
                
                db.set_configuration("rss_feeds", &name, url).await?;
                count += 1;
                info!("Migrated RSS feed: {} -> {}", name, url);
            }
        }
    }
    
    info!("Successfully migrated {} RSS feeds", count);
    Ok(())
}

async fn migrate_decision_workers(db: &Database) -> Result<()> {
    info!("Migrating decision worker configurations...");
    let mut count = 0;
    
    // Migrate DECISION_OLLAMA_CONFIGS
    if let Ok(configs) = env::var("DECISION_OLLAMA_CONFIGS") {
        for (i, config) in configs.split(';').enumerate() {
            let config = config.trim();
            if config.is_empty() { continue; }
            
            let parts: Vec<&str> = config.split('|').collect();
            if parts.len() >= 3 {
                let host = parts[0].trim();
                let port = parts[1].trim().parse::<u16>().unwrap_or(11434);
                let model = parts[2].trim();
                let no_think = model.contains("/no_think");
                let clean_model = model.replace("/no_think", "");
                
                let config_json = json!({
                    "type": "ollama",
                    "host": host,
                    "port": port,
                    "model": clean_model,
                    "no_think": no_think,
                    "enabled": true
                });
                
                let name = format!("ollama_decision_{}", i + 1);
                db.set_configuration("decision_workers", &name, &config_json.to_string()).await?;
                count += 1;
                info!("Migrated decision worker: {} -> {}:{}:{}", name, host, port, clean_model);
            }
        }
    }
    
    // Migrate DECISION_OPENAI_CONFIGS  
    if let Ok(configs) = env::var("DECISION_OPENAI_CONFIGS") {
        for (i, config) in configs.split(';').enumerate() {
            let config = config.trim();
            if config.is_empty() { continue; }
            
            let parts: Vec<&str> = config.split('|').collect();
            if parts.len() >= 2 {
                let api_key = parts[0].trim();
                let model = parts[1].trim();
                
                let config_json = json!({
                    "type": "openai",
                    "api_key": api_key,
                    "model": model,
                    "enabled": true
                });
                
                let name = format!("openai_decision_{}", i + 1);
                db.set_configuration("decision_workers", &name, &config_json.to_string()).await?;
                count += 1;
                info!("Migrated decision worker: {} -> {}", name, model);
            }
        }
    }
    
    info!("Successfully migrated {} decision workers", count);
    Ok(())
}

async fn migrate_analysis_workers(db: &Database) -> Result<()> {
    info!("Migrating analysis worker configurations...");
    let mut count = 0;
    
    // Migrate ANALYSIS_OLLAMA_CONFIGS
    if let Ok(configs) = env::var("ANALYSIS_OLLAMA_CONFIGS") {
        for (i, config) in configs.split(';').enumerate() {
            let config = config.trim();
            if config.is_empty() { continue; }
            
            if config.contains("||") {
                // Dual model configuration (analysis + fallback)
                let parts: Vec<&str> = config.split("||").collect();
                if parts.len() == 2 {
                    let primary: Vec<&str> = parts[0].trim().split('|').collect();
                    let fallback: Vec<&str> = parts[1].trim().split('|').collect();
                    
                    if primary.len() >= 3 && fallback.len() >= 3 {
                        let primary_model = primary[2].trim();
                        let fallback_model = fallback[2].trim();
                        let no_think_primary = primary_model.contains("/no_think");
                        let no_think_fallback = fallback_model.contains("/no_think");
                        
                        let config_json = json!({
                            "type": "ollama",
                            "host": primary[0].trim(),
                            "port": primary[1].trim().parse::<u16>().unwrap_or(11434),
                            "primary_model": primary_model.replace("/no_think", ""),
                            "fallback_model": fallback_model.replace("/no_think", ""),
                            "no_think_primary": no_think_primary,
                            "no_think_fallback": no_think_fallback,
                            "idle_timeout_minutes": 5,
                            "fallback_duration_minutes": 30,
                            "enabled": true
                        });
                        
                        let name = format!("ollama_analysis_dual_{}", i + 1);
                        db.set_configuration("analysis_workers", &name, &config_json.to_string()).await?;
                        count += 1;
                        info!("Migrated dual analysis worker: {} -> {}+{}", name, 
                              primary_model.replace("/no_think", ""), 
                              fallback_model.replace("/no_think", ""));
                    }
                }
            } else {
                // Single model configuration (analysis only)
                let parts: Vec<&str> = config.split('|').collect();
                if parts.len() >= 3 {
                    let host = parts[0].trim();
                    let port = parts[1].trim().parse::<u16>().unwrap_or(11434);
                    let model = parts[2].trim();
                    let no_think = model.contains("/no_think");
                    let clean_model = model.replace("/no_think", "");
                    
                    let config_json = json!({
                        "type": "ollama",
                        "host": host,
                        "port": port,
                        "primary_model": clean_model,
                        "no_think_primary": no_think,
                        "enabled": true
                    });
                    
                    let name = format!("ollama_analysis_single_{}", i + 1);
                    db.set_configuration("analysis_workers", &name, &config_json.to_string()).await?;
                    count += 1;
                    info!("Migrated single analysis worker: {} -> {}:{}", name, host, clean_model);
                }
            }
        }
    }
    
    // Migrate ANALYSIS_OPENAI_CONFIGS
    if let Ok(configs) = env::var("ANALYSIS_OPENAI_CONFIGS") {
        for (i, config) in configs.split(';').enumerate() {
            let config = config.trim();
            if config.is_empty() { continue; }
            
            if config.contains("||") {
                // Dual model configuration
                let parts: Vec<&str> = config.split("||").collect();
                if parts.len() == 2 {
                    let primary: Vec<&str> = parts[0].trim().split('|').collect();
                    let fallback: Vec<&str> = parts[1].trim().split('|').collect();
                    
                    if primary.len() >= 2 && fallback.len() >= 2 {
                        let config_json = json!({
                            "type": "openai",
                            "primary_api_key": primary[0].trim(),
                            "primary_model": primary[1].trim(),
                            "fallback_api_key": fallback[0].trim(),
                            "fallback_model": fallback[1].trim(),
                            "idle_timeout_minutes": 5,
                            "fallback_duration_minutes": 30,
                            "enabled": true
                        });
                        
                        let name = format!("openai_analysis_dual_{}", i + 1);
                        db.set_configuration("analysis_workers", &name, &config_json.to_string()).await?;
                        count += 1;
                        info!("Migrated dual OpenAI analysis worker: {}", name);
                    }
                }
            } else {
                // Single model configuration
                let parts: Vec<&str> = config.split('|').collect();
                if parts.len() >= 2 {
                    let config_json = json!({
                        "type": "openai",
                        "primary_api_key": parts[0].trim(),
                        "primary_model": parts[1].trim(),
                        "enabled": true
                    });
                    
                    let name = format!("openai_analysis_single_{}", i + 1);
                    db.set_configuration("analysis_workers", &name, &config_json.to_string()).await?;
                    count += 1;
                    info!("Migrated single OpenAI analysis worker: {}", name);
                }
            }
        }
    }
    
    info!("Successfully migrated {} analysis workers", count);
    Ok(())
}

async fn migrate_system_settings(db: &Database) -> Result<()> {
    info!("Migrating system settings...");
    let mut count = 0;
    
    let system_vars = [
        ("slack_token", "SLACK_TOKEN"),
        ("slack_channel", "SLACK_CHANNEL"), 
        ("places_json_path", "PLACES_JSON_PATH"),
        ("rust_log", "RUST_LOG"),
    ];
    
    for (key, env_var) in system_vars {
        if let Ok(value) = env::var(env_var) {
            db.set_configuration("system", key, &value).await?;
            count += 1;
            info!("Migrated system setting: {} -> {}", key, 
                  if key == "slack_token" { "[REDACTED]" } else { &value });
        }
    }
    
    info!("Successfully migrated {} system settings", count);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    
    info!("Starting environment configuration migration to database...");
    
    let db = Database::instance().await;
    
    // Run all migrations
    migrate_topics(&db).await?;
    migrate_rss_feeds(&db).await?;
    migrate_decision_workers(&db).await?;
    migrate_analysis_workers(&db).await?;
    migrate_system_settings(&db).await?;
    
    info!("✅ Configuration migration completed successfully!");
    info!("Environment variables have been imported to database.");
    info!("You can now update your services to use database configuration and remove environment variables.");
    
    Ok(())
}
```

### 3.2 Traditional Data Export/Import Scripts

**Create scripts/export_sqlite_data.py:**
```python
#!/usr/bin/env python3
"""
Export data from SQLite to CSV files for PostgreSQL import
"""
import sqlite3
import csv
import os
import json
from datetime import datetime

def export_table_to_csv(db_path, table_name, output_dir):
    """Export a table to CSV"""
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    # Get table schema
    cursor.execute(f"PRAGMA table_info({table_name})")
    columns = [col[1] for col in cursor.fetchall()]
    
    # Export data
    cursor.execute(f"SELECT * FROM {table_name}")
    
    output_file = os.path.join(output_dir, f"{table_name}.csv")
    with open(output_file, 'w', newline='', encoding='utf-8') as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(columns)
        
        for row in cursor.fetchall():
            # Convert timestamps and handle JSON
            converted_row = []
            for i, value in enumerate(row):
                if value is None:
                    converted_row.append('')
                elif columns[i] in ['seen_at', 'pub_date', 'event_date', 'creation_date', 'last_updated']:
                    # Convert to ISO format if it's a timestamp
                    if value and not value.startswith('20'):  # Simple heuristic
                        converted_row.append(value)
                    else:
                        try:
                            # Convert to PostgreSQL timestamp format
                            dt = datetime.fromisoformat(value.replace('Z', '+00:00'))
                            converted_row.append(dt.isoformat())
                        except:
                            converted_row.append(value)
                else:
                    converted_row.append(value)
            
            writer.writerow(converted_row)
    
    conn.close()
    print(f"Exported {table_name} to {output_file}")

def main():
    db_path = 'argus.db'
    output_dir = 'migration_data'
    
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)
    
    # Get all table names
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = [row[0] for row in cursor.fetchall()]
    conn.close()
    
    # Export each table
    for table in tables:
        try:
            export_table_to_csv(db_path, table, output_dir)
        except Exception as e:
            print(f"Error exporting {table}: {e}")

if __name__ == "__main__":
    main()
```

**Create scripts/import_to_postgres.py:**
```python
#!/usr/bin/env python3
"""
Import CSV data into PostgreSQL
"""
import psycopg2
import csv
import os
import sys

def import_csv_to_table(conn, table_name, csv_file):
    """Import CSV file to PostgreSQL table"""
    cursor = conn.cursor()
    
    with open(csv_file, 'r', encoding='utf-8') as f:
        reader = csv.reader(f)
        columns = next(reader)  # Skip header
        
        # Create COPY command
        copy_sql = f"COPY {table_name} ({','.join(columns)}) FROM STDIN WITH CSV"
        
        # Reset file pointer and skip header
        f.seek(0)
        next(reader)
        
        cursor.copy_expert(copy_sql, f)
        conn.commit()
        
        print(f"Imported {csv_file} to {table_name}")

def main():
    # Database connection
    conn = psycopg2.connect(
        host=os.getenv('POSTGRES_HOST', 'localhost'),
        database=os.getenv('POSTGRES_DB', 'argus_prod'),
        user=os.getenv('POSTGRES_USER', 'argus_user'),
        password=os.getenv('POSTGRES_PASSWORD')
    )
    
    migration_dir = 'migration_data'
    
    # Import order (respecting foreign keys)
    import_order = [
        'entities',
        'articles',
        'article_entities',
        'article_clusters',
        'article_cluster_mappings',
        'cluster_merge_history',
        'entity_aliases',
        'rss_queue',
        'matched_topics_queue',
        'life_safety_queue',
        'devices',
        'device_subscriptions',
        'ip_logs',
        'endpoint_timeout_events',
        'endpoint_alerts',
    ]
    
    for table in import_order:
        csv_file = os.path.join(migration_dir, f"{table}.csv")
        if os.path.exists(csv_file):
            try:
                import_csv_to_table(conn, table, csv_file)
            except Exception as e:
                print(f"Error importing {table}: {e}")
                conn.rollback()
        else:
            print(f"Warning: {csv_file} not found, skipping {table}")
    
    # Update sequences
    cursor = conn.cursor()
    for table in import_order:
        try:
            cursor.execute(f"SELECT setval('{table}_id_seq', (SELECT MAX(id) FROM {table}));")
            conn.commit()
        except:
            pass  # Some tables might not have sequences
    
    conn.close()
    print("Data migration completed")

if __name__ == "__main__":
    main()
```

---

## Phase 4: Database Abstraction and Configuration Manager (Day 5)

### 4.1 Enhanced Database Core Implementation

**Update src/db/core.rs:**
```rust
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Postgres, Sqlite, Row,
};
use std::str::FromStr;
use tokio::time::Duration;
use tracing::{info, error};

use super::config::DatabaseConfig;

#[derive(Clone)]
pub enum DatabasePool {
    Sqlite(Pool<Sqlite>),
    Postgres(Pool<Postgres>),
}

#[derive(Clone)]
pub struct Database {
    pool: DatabasePool,
    config: DatabaseConfig,
}

impl Database {
    pub async fn new_from_config(config: DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool = match &config {
            DatabaseConfig::Sqlite { path } => {
                let connect_options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .busy_timeout(Duration::from_secs(5))
                    .synchronous(SqliteSynchronous::Normal);

                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect_with(connect_options)
                    .await?;
                
                DatabasePool::Sqlite(pool)
            }
            
            DatabaseConfig::Postgres { url } => {
                let connect_options = PgConnectOptions::from_str(url)?
                    .application_name("argus");

                let pool = PgPoolOptions::new()
                    .max_connections(20)  // Increased for better concurrency
                    .acquire_timeout(Duration::from_secs(10))
                    .connect_with(connect_options)
                    .await?;
                
                DatabasePool::Postgres(pool)
            }
        };

        let db = Database { pool, config };
        db.initialize_schema().await?;
        Ok(db)
    }

    // Configuration management methods
    pub async fn set_configuration(&self, category: &str, name: &str, value: &str) -> Result<(), sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO configurations (category, name, value, updated_at)
                    VALUES ($1, $2, $3, NOW())
                    ON CONFLICT (category, name) 
                    DO UPDATE SET value = $3, updated_at = NOW()
                    "#
                )
                .bind(category)
                .bind(name)
                .bind(value)
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO configurations (category, name, value, updated_at)
                    VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)
                    ON CONFLICT (category, name) 
                    DO UPDATE SET value = ?3, updated_at = CURRENT_TIMESTAMP
                    "#
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

    pub async fn get_configuration(&self, category: &str, name: &str) -> Result<Option<String>, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query_scalar::<_, String>(
                    "SELECT value FROM configurations WHERE category = $1 AND name = $2 AND enabled = true"
                )
                .bind(category)
                .bind(name)
                .fetch_optional(pool)
                .await?;
                Ok(result)
            }
            DatabasePool::Sqlite(pool) => {
                let result = sqlx::query_scalar::<_, String>(
                    "SELECT value FROM configurations WHERE category = ?1 AND name = ?2 AND enabled = true"
                )
                .bind(category)
                .bind(name)
                .fetch_optional(pool)
                .await?;
                Ok(result)
            }
        }
    }

    pub async fn get_configurations_by_category(&self, category: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT name, value FROM configurations WHERE category = $1 AND enabled = true ORDER BY name"
                )
                .bind(category)
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (row.get::<String, _>("name"), row.get::<String, _>("value"))
                }).collect())
            }
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT name, value FROM configurations WHERE category = ?1 AND enabled = true ORDER BY name"
                )
                .bind(category)
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (row.get::<String, _>("name"), row.get::<String, _>("value"))
                }).collect())
            }
        }
    }

    pub async fn delete_configuration(&self, category: &str, name: &str) -> Result<(), sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query("DELETE FROM configurations WHERE category = $1 AND name = $2")
                    .bind(category)
                    .bind(name)
                    .execute(pool)
                    .await?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query("DELETE FROM configurations WHERE category = ?1 AND name = ?2")
                    .bind(category)
                    .bind(name)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn get_all_configurations(&self) -> Result<Vec<(String, String, String)>, sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT category, name, value FROM configurations WHERE enabled = true ORDER BY category, name"
                )
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (
                        row.get::<String, _>("category"),
                        row.get::<String, _>("name"),
                        row.get::<String, _>("value")
                    )
                }).collect())
            }
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(
                    "SELECT category, name, value FROM configurations WHERE enabled = true ORDER BY category, name"
                )
                .fetch_all(pool)
                .await?;
                
                Ok(rows.into_iter().map(|row| {
                    (
                        row.get::<String, _>("category"),
                        row.get::<String, _>("name"),
                        row.get::<String, _>("value")
                    )
                }).collect())
            }
        }
    }

    pub(crate) async fn initialize_schema(&self) -> Result<(), sqlx::Error> {
        match &self.pool {
            DatabasePool::Postgres(pool) => {
                // Read and execute PostgreSQL schema
                let schema_sql = include_str!("../../migrations/postgresql_schema.sql");
                sqlx::query(schema_sql).execute(pool).await?;
                info!(target: "db", "PostgreSQL schema initialized");
            }
            DatabasePool::Sqlite(pool) => {
                // Existing SQLite schema code
                // ... (keep existing SQLite implementation)
                info!(target: "db", "SQLite schema initialized");
            }
        }
        Ok(())
    }
}
```

### 4.2 Configuration Manager Implementation

**Create src/config/manager.rs:**
```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use anyhow::Result;
use serde_json;

use crate::db::Database;
use super::types::*;

pub struct ConfigManager {
    cache: Arc<RwLock<HashMap<String, String>>>,
    db: Arc<Database>,
}

impl ConfigManager {
    pub async fn new() -> Result<Self> {
        let db = Arc::new(Database::instance().await);
        let cache = Arc::new(RwLock::new(HashMap::new()));
        
        let manager = ConfigManager { cache, db };
        manager.refresh_cache().await?;
        
        info!("Configuration manager initialized with {} cached items", 
              manager.cache.read().await.len());
        
        Ok(manager)
    }
    
    async fn refresh_cache(&self) -> Result<()> {
        info!("Refreshing configuration cache from database...");
        
        let configs = self.db.get_all_configurations().await
            .map_err(|e| anyhow::anyhow!("Failed to load configurations: {}", e))?;
        
        let mut cache = self.cache.write().await;
        cache.clear();
        
        for (category, name, value) in configs {
            let key = format!("{}:{}", category, name);
            cache.insert(key, value);
        }
        
        info!("Configuration cache refreshed with {} items", cache.len());
        Ok(())
    }
    
    pub async fn get_topics(&self) -> Vec<TopicConfig> {
        let cache = self.cache.read().await;
        cache.iter()
            .filter(|(k, _)| k.starts_with("topics:"))
            .map(|(k, v)| TopicConfig {
                name: k.strip_prefix("topics:").unwrap().to_string(),
                prompt: v.clone(),
                enabled: true, // All items in cache are enabled
            })
            .collect()
    }
    
    pub async fn get_rss_feeds(&self) -> Vec<RssFeedConfig> {
        let cache = self.cache.read().await;
        cache.iter()
            .filter(|(k, _)| k.starts_with("rss_feeds:"))
            .map(|(k, v)| RssFeedConfig {
                name: k.strip_prefix("rss_feeds:").unwrap().to_string(),
                url: v.clone(),
                enabled: true,
            })
            .collect()
    }
    
    pub async fn get_system_setting(&self, key: &str) -> Option<String> {
        let cache_key = format!("system:{}", key);
        let cache = self.cache.read().await;
        cache.get(&cache_key).cloned()
    }
    
    pub async fn add_topic(&self, name: &str, prompt: &str) -> Result<()> {
        self.db.set_configuration("topics", name, prompt).await
            .map_err(|e| anyhow::anyhow!("Failed to add topic: {}", e))?;
        self.refresh_cache().await?;
        info!("Added topic: {} -> {}", name, prompt);
        Ok(())
    }
    
    pub async fn remove_topic(&self, name: &str) -> Result<()> {
        self.db.delete_configuration("topics", name).await
            .map_err(|e| anyhow::anyhow!("Failed to remove topic: {}", e))?;
        self.refresh_cache().await?;
        info!("Removed topic: {}", name);
        Ok(())
    }
}
```

---

## Phase 5: Application Integration (Day 6)

### 5.1 Update Main Application Entry Point

**Update src/main.rs:**
```rust
use argus::config::ConfigManager;
use anyhow::Result;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    
    info!("Starting Argus with database-driven configuration...");
    
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
    
    info!("Loaded configuration: {} topics, {} RSS feeds",
          topics.len(), rss_feeds.len());
    
    // Convert to format expected by existing worker code
    let topic_strings: Vec<String> = topics.into_iter()
        .map(|t| format!("{}:{}", t.name, t.prompt))
        .collect();
    
    let rss_urls: Vec<String> = rss_feeds.into_iter()
        .map(|f| f.url)
        .collect();
    
    // Start application components with database configuration
    start_application_with_config(
        topic_strings,
        rss_urls,
        slack_token,
        slack_channel
    ).await?;
    
    Ok(())
}

async fn start_application_with_config(
    topics: Vec<String>,
    rss_urls: Vec<String>,
    slack_token: String,
    slack_channel: String,
) -> Result<()> {
    // Implementation adapted from existing main.rs
    // ... (start workers, API server, etc. using database configuration)
    Ok(())
}
```

### 5.2 Enhanced API with Configuration Management

**Update src/app/api.rs to include configuration endpoints:**
```rust
use argus::config::{ConfigManager, TopicConfig, RssFeedConfig};

// Add configuration management request types
#[derive(Deserialize)]
struct CreateTopicRequest {
    name: String,
    prompt: String,
}

#[derive(Deserialize)]
struct CreateRssFeedRequest {
    name: String,
    url: String,
}

// Configuration management endpoints
async fn get_admin_topics(
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
) -> Result<Json<Vec<TopicConfig>>, StatusCode> {
    // Validate admin JWT
    let token = auth_header.token();
    if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    let config_manager = ConfigManager::new().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let topics = config_manager.get_topics().await;
    
    Ok(Json(topics))
}

async fn add_topic(
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<CreateTopicRequest>,
) -> Result<StatusCode, StatusCode> {
    // Validate admin JWT
    let token = auth_header.token();
    if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    let config_manager = ConfigManager::new().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    config_manager.add_topic(&payload.name, &payload.prompt).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::CREATED)
}

async fn delete_topic(
    TypedHeader(auth_header): TypedHeader<Authorization<Bearer>>,
    Path(topic_name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Validate admin JWT
    let token = auth_header.token();
    if decode::<Claims>(token, &DECODING_KEY, &Validation::new(Algorithm::HS256)).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    let config_manager = ConfigManager::new().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    config_manager.remove_topic(&topic_name).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::OK)
}

// Update main router to include configuration endpoints
pub async fn app_api_loop() -> Result<()> {
    let app = Router::new()
        .route("/status", post(status_check))
        .route("/authenticate", post(authenticate))
        .route("/subscriptions", post(get_subscriptions))
        .route("/subscribe", post(subscribe_to_topic))
        .route("/unsubscribe", post(unsubscribe_from_topic))
        .route("/articles/sync", post(sync_seen_articles))
        .route("/articles/analyze-match", post(analyze_article_match))
        .route("/clusters/sync", post(sync_clusters))
        // New configuration management endpoints
        .route("/admin/topics", get(get_admin_topics).post(add_topic))
        .route("/admin/topics/:name", delete(delete_topic));

    // ... rest of the existing implementation
    Ok(())
}
```

---

## Phase 6: Testing and Validation (Day 7)

### 6.1 Migration Testing Scripts

**Create scripts/test_migration.sh:**
```bash
#!/bin/bash

# Test script for validating migration
set -e

echo "=== Testing PostgreSQL Migration ==="

# Create test database
echo "Setting up test environment..."
createdb argus_test || true

# Set test environment
export DATABASE_TYPE=postgres
export DATABASE_URL=postgresql://argus_user:password@localhost/argus_test

echo "Testing schema creation..."
cargo run --bin migrate_postgresql_schema

echo "Testing configuration migration..."
cargo run --bin migrate_env_to_db

echo "Testing basic database operations..."
cargo test --test postgres_migration_test

echo "Testing API with database configuration..."
# Start API server in background
cargo run --bin argus-api &
API_PID=$!
sleep 5

# Test API endpoints
curl -X POST http://localhost:8080/status || echo "API test failed"

# Cleanup
kill $API_PID

echo "✅ Migration tests completed successfully!"
```

### 6.2 Performance Benchmarking

**Create scripts/benchmark_comparison.py:**
```python
#!/usr/bin/env python3
"""
Compare performance between SQLite and PostgreSQL
"""
import time
import subprocess
import statistics
import os

def run_benchmark(db_type, operation):
    """Run a benchmark operation"""
    env = os.environ.copy()
    env['DATABASE_TYPE'] = db_type
    
    start_time = time.time()
    result = subprocess.run([
        'cargo', 'run', '--bin', f'benchmark_{operation}'
    ], env=env, capture_output=True, text=True)
    end_time = time.time()
    
    return end_time - start_time, result.returncode == 0

def main():
    operations = ['concurrent_writes', 'large_queries', 'config_access']
    results = {}
    
    for operation in operations:
        print(f"Benchmarking {operation}...")
        
        # SQLite benchmark
        sqlite_times = []
        for _ in range(5):
            time_taken, success = run_benchmark('sqlite', operation)
            if success:
                sqlite_times.append(time_taken)
        
        # PostgreSQL benchmark
        postgres_times = []
        for _ in range(5):
            time_taken, success = run_benchmark('postgres', operation)
            if success:
                postgres_times.append(time_taken)
        
        results[operation] = {
            'sqlite': statistics.mean(sqlite_times) if sqlite_times else None,
            'postgres': statistics.mean(postgres_times) if postgres_times else None
        }
    
    # Print results
    print("\nBenchmark Results:")
    print("-" * 50)
    for operation, times in results.items():
        print(f"{operation}:")
        if times['sqlite']:
            print(f"  SQLite:     {times['sqlite']:.3f}s")
        if times['postgres']:
            print(f"  PostgreSQL: {times['postgres']:.3f}s")
        if times['sqlite'] and times['postgres']:
            improvement = ((times['sqlite'] - times['postgres']) / times['sqlite']) * 100
            print(f"  Improvement: {improvement:.1f}%")
        print()

if __name__ == "__main__":
    main()
```

---

## Phase 7: Production Deployment (Day 8)

### 7.1 Deployment Checklist

**Pre-deployment:**
- [ ] PostgreSQL server configured with appropriate resources
- [ ] Database user created with proper permissions
- [ ] Connection pooling configured (pgbouncer recommended)
- [ ] Backup strategy in place
- [ ] Monitoring setup (pg_stat_statements, etc.)

**Deployment Steps:**
1. **Stop all Argus services**
   ```bash
   systemctl stop argus-*
   ```

2. **Final SQLite backup**
   ```bash
   cp argus.db argus.db.final_backup.$(date +%Y%m%d_%H%M%S)
   ```

3. **Run environment migration**
   ```bash
   # Set PostgreSQL environment
   export DATABASE_TYPE=postgres
   export DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod
   
   # Run configuration migration
   cargo run --release --bin migrate_env_to_db
   ```

4. **Run data migration**
   ```bash
   python3 scripts/export_sqlite_data.py
   python3 scripts/import_to_postgres.py
   ```

5. **Start services with PostgreSQL**
   ```bash
   systemctl start argus-api
   systemctl start argus-workers
   ```

6. **Validate operation**
   ```bash
   # Check service status
   systemctl status argus-*
   
   # Test API endpoints
   curl http://localhost:8080/status
   
   # Monitor logs
   journalctl -f -u argus-api
   ```

### 7.2 Rollback Plan

**If issues occur:**
1. **Stop PostgreSQL services**
   ```bash
   systemctl stop argus-*
   ```

2. **Switch back to SQLite**
   ```bash
   export DATABASE_TYPE=sqlite
   export DATABASE_PATH=argus.db
   ```

3. **Restore from backup if needed**
   ```bash
   cp argus.db.final_backup.YYYYMMDD_HHMMSS argus.db
   ```

4. **Restart services**
   ```bash
   systemctl start argus-*
   ```

---

## Post-Migration Optimization

### PostgreSQL Configuration
```ini
# postgresql.conf optimizations
shared_buffers = 256MB
work_mem = 4MB
maintenance_work_mem = 64MB
max_connections = 100
wal_buffers = 16MB
checkpoint_completion_target = 0.9
random_page_cost = 1.1
effective_cache_size = 1GB
```

### Configuration Management
- **Runtime topic management**: Add/remove topics without restarts
- **Feed management**: Enable/disable RSS feeds dynamically
- **Worker scaling**: Adjust worker configurations on the fly
- **System settings**: Update Slack channels, logging levels, etc.

---

## Success Metrics

### Performance Improvements
- **Elimination of "database is locked" errors**
- **20-50% improvement in concurrent operation performance**
- **Better handling of high-frequency updates**
- **Improved API response times under load**

### Configuration Benefits
- **Runtime topic management**: Add new topics instantly
- **Dynamic RSS feed management**: No restarts for feed changes
- **Centralized configuration**: Single source of truth
- **Audit trail**: Track all configuration changes

### Operational Benefits
- **Scalable architecture**: Ready for public service deployment
- **Better monitoring**: PostgreSQL statistics and logging
- **Improved reliability**: No more SQLite concurrency issues
- **Enhanced maintainability**: Database-driven configuration

This comprehensive migration plan provides a complete roadmap for transforming Argus from a POC with hardcoded configuration to a production-ready system with dynamic, database-driven configuration and PostgreSQL scalability.
