# 03 - Schema Migration

**Effort**: M (2 days)

## PostgreSQL Schema Design

### Configuration Tables
```sql
-- Core configuration storage
CREATE TABLE configurations (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    value TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(category, name)
);

-- Configuration change audit
CREATE TABLE configuration_changes (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    old_value TEXT,
    new_value TEXT,
    changed_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_configurations_category ON configurations(category);
CREATE INDEX idx_configurations_enabled ON configurations(category, enabled);
```

### Core Tables (PostgreSQL Compatible)
```sql
-- Articles table
CREATE TABLE articles (
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
    analysis JSONB,  -- Changed from TEXT to JSONB for better performance
    json_data JSONB,
    quality REAL,
    hash TEXT,
    title_domain_hash TEXT,
    r2_url TEXT,
    cluster_id BIGINT
);

-- Article indexes
CREATE INDEX idx_relevant_category ON articles (is_relevant, category);
CREATE INDEX idx_hash ON articles (hash);
CREATE INDEX idx_title_domain_hash ON articles (title_domain_hash);
CREATE INDEX idx_articles_cluster_id ON articles (cluster_id);
CREATE INDEX idx_articles_event_date ON articles (event_date);

-- JSONB indexes for better performance
CREATE INDEX idx_articles_analysis_gin ON articles USING GIN (analysis);
```

### Entity Tables
```sql
-- Entities
CREATE TABLE entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    parent_id BIGINT,
    UNIQUE(normalized_name, type),
    FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX idx_entities_normalized_name ON entities (normalized_name);
CREATE INDEX idx_entities_type ON entities (type);

-- Article-Entity relationships
CREATE TABLE article_entities (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    entity_id BIGINT NOT NULL,
    importance TEXT NOT NULL,
    context TEXT,
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
    UNIQUE(article_id, entity_id)
);

-- Entity aliases
CREATE TABLE entity_aliases (
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
```

### Clustering Tables
```sql
-- Article clusters
CREATE TABLE article_clusters (
    id BIGSERIAL PRIMARY KEY,
    name TEXT,
    creation_date TIMESTAMPTZ NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL,
    primary_entity_ids JSONB NOT NULL DEFAULT '[]',
    article_count INTEGER NOT NULL DEFAULT 0,
    summary TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    importance_score REAL NOT NULL DEFAULT 0.0
);

-- Cluster mappings
CREATE TABLE article_cluster_mappings (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    cluster_id BIGINT NOT NULL,
    similarity_score REAL NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
    UNIQUE(article_id, cluster_id)
);

-- Merge history
CREATE TABLE cluster_merge_history (
    id BIGSERIAL PRIMARY KEY,
    original_cluster_id BIGINT NOT NULL,
    merged_into_cluster_id BIGINT NOT NULL,
    merge_reason TEXT,
    merged_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);
```

### Queue Tables
```sql
-- RSS queue
CREATE TABLE rss_queue (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    title TEXT,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

-- Matched topics queue
CREATE TABLE matched_topics_queue (
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

-- Life safety queue
CREATE TABLE life_safety_queue (
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
```

### Device & Monitoring Tables
```sql
-- Devices
CREATE TABLE devices (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE,
    device_type TEXT,
    app_version TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_seen TIMESTAMPTZ DEFAULT NOW()
);

-- Device subscriptions
CREATE TABLE device_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL,
    topic TEXT NOT NULL,
    priority TEXT,
    FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
    UNIQUE(device_id, topic)
);

-- Endpoint monitoring
CREATE TABLE endpoint_timeout_events (
    id BIGSERIAL PRIMARY KEY,
    endpoint_name TEXT NOT NULL,
    timeout_duration INTEGER NOT NULL,
    error_message TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE endpoint_alerts (
    id BIGSERIAL PRIMARY KEY,
    endpoint_name TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    is_resolved BOOLEAN DEFAULT FALSE
);
```

## Schema Creation Binary

**Create migrations/postgresql_schema.sql** with complete schema above.

**Create src/bin/create_postgres_schema.rs:**
```rust
use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    let schema_sql = include_str!("../../migrations/postgresql_schema.sql");
    sqlx::query(schema_sql).execute(&pool).await?;
    
    println!("✅ PostgreSQL schema created successfully");
    Ok(())
}
```

## Execution Steps
1. Create `migrations/postgresql_schema.sql` with complete schema
2. Build schema creation binary: `cargo build --bin create_postgres_schema`
3. Run schema creation: `cargo run --bin create_postgres_schema`
4. Verify tables created: `psql $DATABASE_URL -c "\dt"`
