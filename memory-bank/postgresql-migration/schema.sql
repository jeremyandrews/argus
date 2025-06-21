-- PostgreSQL Schema for Argus
-- Generated from src/db/schema.rs (source of truth)
-- This schema includes ALL tables from the SQLite database

-- Core Articles Table
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
    analysis TEXT,
    json_data TEXT,
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
CREATE INDEX idx_r2_url ON articles (r2_url);
CREATE INDEX idx_seen_at_r2_url ON articles (seen_at, r2_url);
CREATE INDEX idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url);
CREATE INDEX idx_articles_event_date ON articles (event_date);
CREATE INDEX idx_articles_pub_date ON articles (pub_date);
CREATE INDEX idx_articles_cluster_id ON articles (cluster_id);

-- Entity tables for improved article matching
CREATE TABLE entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL, -- PERSON, ORGANIZATION, LOCATION, EVENT, etc.
    normalized_name TEXT NOT NULL, -- For easier matching
    parent_id BIGINT, -- For hierarchical relations (especially locations)
    UNIQUE(normalized_name, type),
    FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX idx_entities_normalized_name ON entities (normalized_name);
CREATE INDEX idx_entities_type ON entities (type);
CREATE INDEX idx_entities_parent_id ON entities (parent_id);

-- Entity-Article relationships
CREATE TABLE article_entities (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    entity_id BIGINT NOT NULL,
    importance TEXT NOT NULL, -- PRIMARY, SECONDARY, MENTIONED
    context TEXT, -- Additional context about the entity in this article
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
    UNIQUE(article_id, entity_id)
);

CREATE INDEX idx_article_entities_article_id ON article_entities (article_id);
CREATE INDEX idx_article_entities_entity_id ON article_entities (entity_id);
CREATE INDEX idx_article_entities_importance ON article_entities (importance);

-- Article clusters
CREATE TABLE article_clusters (
    id BIGSERIAL PRIMARY KEY,
    name TEXT,
    creation_date TIMESTAMPTZ NOT NULL,
    last_updated TIMESTAMPTZ NOT NULL,
    primary_entity_ids TEXT NOT NULL DEFAULT '[]',
    article_count INTEGER NOT NULL DEFAULT 0,
    needs_summary_update INTEGER NOT NULL DEFAULT 1,
    summary TEXT,
    summary_version INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    importance_score REAL NOT NULL DEFAULT 0.0,
    has_timeline INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_article_clusters_last_updated ON article_clusters (last_updated);
CREATE INDEX idx_article_clusters_status ON article_clusters (status);
CREATE INDEX idx_article_clusters_importance ON article_clusters (importance_score);

-- Article-cluster relationships
CREATE TABLE article_cluster_mappings (
    id BIGSERIAL PRIMARY KEY,
    article_id BIGINT NOT NULL,
    cluster_id BIGINT NOT NULL,
    added_date TIMESTAMPTZ NOT NULL,
    similarity_score REAL,
    FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
    UNIQUE(article_id, cluster_id)
);

CREATE INDEX idx_article_cluster_mappings_article_id ON article_cluster_mappings (article_id);
CREATE INDEX idx_article_cluster_mappings_cluster_id ON article_cluster_mappings (cluster_id);

-- Cluster merge history
CREATE TABLE cluster_merge_history (
    id BIGSERIAL PRIMARY KEY,
    original_cluster_id BIGINT NOT NULL,
    merged_into_cluster_id BIGINT NOT NULL,
    merge_date TIMESTAMPTZ NOT NULL,
    merge_reason TEXT,
    FOREIGN KEY (original_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
    FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
);

CREATE INDEX idx_cluster_merge_original ON cluster_merge_history (original_cluster_id);
CREATE INDEX idx_cluster_merge_destination ON cluster_merge_history (merged_into_cluster_id);

-- Entity alias system tables
CREATE TABLE entity_aliases (
    id BIGSERIAL PRIMARY KEY,
    entity_id BIGINT,
    canonical_name TEXT NOT NULL,
    alias_text TEXT NOT NULL,
    normalized_canonical TEXT NOT NULL,
    normalized_alias TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    source TEXT NOT NULL, -- STATIC, PATTERN, LLM, ADMIN, etc.
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'APPROVED', -- APPROVED, PENDING, REJECTED
    UNIQUE (normalized_canonical, normalized_alias, entity_type),
    FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX idx_entity_aliases_canonical ON entity_aliases(normalized_canonical, entity_type);
CREATE INDEX idx_entity_aliases_alias ON entity_aliases(normalized_alias, entity_type);
CREATE INDEX idx_entity_aliases_status ON entity_aliases(status);
CREATE INDEX idx_entity_aliases_source ON entity_aliases(source);

-- Negative match table for explicitly rejected pairs
CREATE TABLE entity_negative_matches (
    id BIGSERIAL PRIMARY KEY,
    entity_id1 BIGINT,
    entity_id2 BIGINT,
    normalized_name1 TEXT NOT NULL,
    normalized_name2 TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    rejected_by TEXT NOT NULL,
    rejected_at TIMESTAMPTZ NOT NULL,
    rejection_reason TEXT,
    persistence_level INTEGER NOT NULL DEFAULT 1,
    UNIQUE (normalized_name1, normalized_name2, entity_type),
    FOREIGN KEY (entity_id1) REFERENCES entities (id) ON DELETE SET NULL,
    FOREIGN KEY (entity_id2) REFERENCES entities (id) ON DELETE SET NULL
);

CREATE INDEX idx_negative_matches_names ON entity_negative_matches(normalized_name1, normalized_name2);
CREATE INDEX idx_negative_matches_type ON entity_negative_matches(entity_type);

-- Alias pattern performance tracking
CREATE TABLE alias_pattern_stats (
    pattern_id TEXT PRIMARY KEY,
    pattern_type TEXT NOT NULL, -- REGEX, LLM, TRANSFORMATION
    total_suggestions INTEGER NOT NULL DEFAULT 0,
    approved_count INTEGER NOT NULL DEFAULT 0,
    rejected_count INTEGER NOT NULL DEFAULT 0,
    last_used_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

-- For batch review in admin interface
CREATE TABLE alias_review_batches (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    admin_id TEXT,
    status TEXT NOT NULL DEFAULT 'OPEN', -- OPEN, COMPLETED
    total_count INTEGER NOT NULL DEFAULT 0,
    processed_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE alias_review_items (
    id BIGSERIAL PRIMARY KEY,
    batch_id BIGINT NOT NULL,
    alias_id BIGINT NOT NULL,
    decision TEXT, -- APPROVED, REJECTED, IGNORED
    decided_at TIMESTAMPTZ,
    FOREIGN KEY (batch_id) REFERENCES alias_review_batches(id),
    FOREIGN KEY (alias_id) REFERENCES entity_aliases(id)
);

-- Cache statistics for optimization
CREATE TABLE alias_cache_stats (
    normalized_name TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    hit_count INTEGER NOT NULL DEFAULT 0,
    last_accessed TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (normalized_name, entity_type)
);

-- RSS Queue
CREATE TABLE rss_queue (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL UNIQUE,
    title TEXT,
    seen_at TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
CREATE UNIQUE INDEX idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url);

-- Matched Topics Queue
CREATE TABLE matched_topics_queue (
    id BIGSERIAL PRIMARY KEY,
    article_text TEXT NOT NULL,
    article_html TEXT NOT NULL,
    article_url TEXT NOT NULL UNIQUE,
    article_title TEXT NOT NULL,
    topic_matched TEXT NOT NULL,
    article_hash TEXT NOT NULL,
    title_domain_hash TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX idx_matched_topics_article_url ON matched_topics_queue (article_url);

-- Life Safety Queue
CREATE TABLE life_safety_queue (
    id BIGSERIAL PRIMARY KEY,
    article_url TEXT NOT NULL UNIQUE,
    article_title TEXT NOT NULL,
    article_text TEXT NOT NULL,
    article_html TEXT NOT NULL,
    article_hash TEXT NOT NULL,
    title_domain_hash TEXT NOT NULL,
    threat TEXT,
    timestamp TIMESTAMPTZ NOT NULL,
    pub_date TIMESTAMPTZ
);

CREATE UNIQUE INDEX idx_life_safety_article_url ON life_safety_queue (article_url);

-- Devices
CREATE TABLE devices (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE
);

CREATE INDEX idx_devices_device_id ON devices (device_id);

-- Device Subscriptions
CREATE TABLE device_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL,
    topic TEXT NOT NULL,
    priority TEXT,
    FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
    UNIQUE(device_id, topic)
);

CREATE INDEX idx_topic_device_id ON device_subscriptions (topic, device_id);
CREATE INDEX idx_device_subscriptions_device_id_topic ON device_subscriptions (device_id, topic);

-- IP Logs
CREATE TABLE ip_logs (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL,
    ip_address TEXT NOT NULL,
    first_seen BIGINT NOT NULL,
    last_seen BIGINT NOT NULL,
    FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
    UNIQUE (device_id, ip_address)
);

CREATE INDEX idx_ip_logs_device_id ON ip_logs (device_id);
CREATE INDEX idx_ip_logs_ip_address ON ip_logs (ip_address);

-- Alert system tables for LLM endpoint monitoring
CREATE TABLE endpoint_timeout_events (
    id BIGSERIAL PRIMARY KEY,
    endpoint_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    worker_type TEXT NOT NULL,
    timeout_type TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_timeout_events_endpoint_model ON endpoint_timeout_events (endpoint_url, model_name);
CREATE INDEX idx_timeout_events_occurred_at ON endpoint_timeout_events (occurred_at);
CREATE INDEX idx_timeout_events_worker ON endpoint_timeout_events (worker_id, worker_type);

CREATE TABLE endpoint_alerts (
    id BIGSERIAL PRIMARY KEY,
    endpoint_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    first_occurrence TIMESTAMPTZ NOT NULL,
    last_occurrence TIMESTAMPTZ NOT NULL,
    last_alert_sent TIMESTAMPTZ,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    consecutive_failures INTEGER NOT NULL DEFAULT 1,
    is_resolved BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_at TIMESTAMPTZ
);

CREATE INDEX idx_endpoint_alerts_endpoint_model ON endpoint_alerts (endpoint_url, model_name);
CREATE INDEX idx_endpoint_alerts_type_resolved ON endpoint_alerts (alert_type, is_resolved);
CREATE INDEX idx_endpoint_alerts_occurrence ON endpoint_alerts (last_occurrence);

-- Configuration Management Table (for database-driven configuration)
CREATE TABLE configurations (
    id BIGSERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,        -- 'topics', 'rss', 'system', etc.
    name VARCHAR(100) NOT NULL,
    value TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(category, name)
);

CREATE INDEX idx_configurations_category ON configurations(category);
CREATE INDEX idx_configurations_enabled ON configurations(category, enabled);
CREATE INDEX idx_configurations_updated ON configurations(updated_at);

-- Update timestamp trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply update trigger to configurations
CREATE TRIGGER update_configurations_updated_at 
    BEFORE UPDATE ON configurations 
    FOR EACH ROW 
    EXECUTE FUNCTION update_updated_at_column();
