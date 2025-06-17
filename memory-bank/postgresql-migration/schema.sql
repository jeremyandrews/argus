-- PostgreSQL Schema for Argus
-- This schema replaces SQLite with PostgreSQL-optimized tables and indexes

-- Configuration Management Tables
CREATE TABLE configurations (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,        -- 'topics', 'rss', 'system'
    name VARCHAR(100) NOT NULL,
    value TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(category, name)
);

-- Configuration change audit trail
CREATE TABLE configuration_changes (
    id SERIAL PRIMARY KEY,
    category VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    old_value TEXT,
    new_value TEXT,
    changed_at TIMESTAMPTZ DEFAULT NOW()
);

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
    analysis JSONB,                       -- Enhanced from TEXT to JSONB for better performance
    json_data JSONB,
    quality REAL,
    hash TEXT,
    title_domain_hash TEXT,
    r2_url TEXT,
    cluster_id BIGINT
);

-- Entities Table
CREATE TABLE entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    parent_id BIGINT,
    UNIQUE(normalized_name, type),
    FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
);

-- Article-Entity Relationships
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

-- Entity Aliases
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

-- Article Clusters
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

-- Article Cluster Mappings
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

-- Cluster Merge History
CREATE TABLE cluster_merge_history (
    id BIGSERIAL PRIMARY KEY,
    original_cluster_id BIGINT NOT NULL,
    merged_into_cluster_id BIGINT NOT NULL,
    merge_reason TEXT,
    merged_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
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

-- Matched Topics Queue
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

-- Life Safety Queue
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

-- Devices
CREATE TABLE devices (
    id BIGSERIAL PRIMARY KEY,
    device_id TEXT NOT NULL UNIQUE,
    device_type TEXT,
    app_version TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    last_seen TIMESTAMPTZ DEFAULT NOW()
);

-- Device Subscriptions
CREATE TABLE device_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL,
    topic TEXT NOT NULL,
    priority TEXT,
    FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
    UNIQUE(device_id, topic)
);

-- Endpoint Monitoring
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

-- Indexes for Performance Optimization

-- Configuration indexes
CREATE INDEX idx_configurations_category ON configurations(category);
CREATE INDEX idx_configurations_enabled ON configurations(category, enabled);
CREATE INDEX idx_configurations_updated ON configurations(updated_at);

-- Article indexes
CREATE INDEX idx_articles_relevant_category ON articles (is_relevant, category);
CREATE INDEX idx_articles_hash ON articles (hash);
CREATE INDEX idx_articles_title_domain_hash ON articles (title_domain_hash);
CREATE INDEX idx_articles_cluster_id ON articles (cluster_id);
CREATE INDEX idx_articles_event_date ON articles (event_date);
CREATE INDEX idx_articles_seen_at ON articles (seen_at);
CREATE INDEX idx_articles_normalized_url ON articles (normalized_url);

-- JSONB indexes for better performance
CREATE INDEX idx_articles_analysis_gin ON articles USING GIN (analysis);
CREATE INDEX idx_articles_json_data_gin ON articles USING GIN (json_data);

-- Entity indexes
CREATE INDEX idx_entities_normalized_name ON entities (normalized_name);
CREATE INDEX idx_entities_type ON entities (type);
CREATE INDEX idx_entities_parent_id ON entities (parent_id);

-- Article-entity relationship indexes
CREATE INDEX idx_article_entities_article_id ON article_entities (article_id);
CREATE INDEX idx_article_entities_entity_id ON article_entities (entity_id);
CREATE INDEX idx_article_entities_importance ON article_entities (importance);

-- Entity alias indexes
CREATE INDEX idx_entity_aliases_entity_id ON entity_aliases (entity_id);
CREATE INDEX idx_entity_aliases_normalized ON entity_aliases (normalized_alias);
CREATE INDEX idx_entity_aliases_confidence ON entity_aliases (confidence);

-- Cluster indexes
CREATE INDEX idx_article_clusters_status ON article_clusters (status);
CREATE INDEX idx_article_clusters_creation_date ON article_clusters (creation_date);
CREATE INDEX idx_article_clusters_importance ON article_clusters (importance_score);

-- Cluster mapping indexes
CREATE INDEX idx_cluster_mappings_article_id ON article_cluster_mappings (article_id);
CREATE INDEX idx_cluster_mappings_cluster_id ON article_cluster_mappings (cluster_id);
CREATE INDEX idx_cluster_mappings_similarity ON article_cluster_mappings (similarity_score);

-- Queue indexes
CREATE INDEX idx_rss_queue_normalized_url ON rss_queue (normalized_url);
CREATE INDEX idx_rss_queue_seen_at ON rss_queue (seen_at);

CREATE INDEX idx_matched_topics_hash ON matched_topics_queue (article_hash);
CREATE INDEX idx_matched_topics_timestamp ON matched_topics_queue (timestamp);
CREATE INDEX idx_matched_topics_topic ON matched_topics_queue (topic_matched);

CREATE INDEX idx_life_safety_hash ON life_safety_queue (article_hash);
CREATE INDEX idx_life_safety_timestamp ON life_safety_queue (timestamp);
CREATE INDEX idx_life_safety_threat ON life_safety_queue (threat_type);

-- Device indexes
CREATE INDEX idx_devices_device_id ON devices (device_id);
CREATE INDEX idx_devices_last_seen ON devices (last_seen);

CREATE INDEX idx_device_subscriptions_device_id ON device_subscriptions (device_id);
CREATE INDEX idx_device_subscriptions_topic ON device_subscriptions (topic);

-- Monitoring indexes
CREATE INDEX idx_timeout_events_endpoint ON endpoint_timeout_events (endpoint_name);
CREATE INDEX idx_timeout_events_timestamp ON endpoint_timeout_events (timestamp);

CREATE INDEX idx_alerts_endpoint ON endpoint_alerts (endpoint_name);
CREATE INDEX idx_alerts_resolved ON endpoint_alerts (is_resolved);
CREATE INDEX idx_alerts_created ON endpoint_alerts (created_at);

-- Functions and Triggers for Enhanced Functionality

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

-- Configuration change audit trigger
CREATE OR REPLACE FUNCTION audit_configuration_changes()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.value != NEW.value THEN
            INSERT INTO configuration_changes (category, name, old_value, new_value, changed_at)
            VALUES (NEW.category, NEW.name, OLD.value, NEW.value, NOW());
        END IF;
        RETURN NEW;
    END IF;
    RETURN NULL;
END;
$$ language 'plpgsql';

-- Apply audit trigger to configurations
CREATE TRIGGER audit_configurations_trigger
    AFTER UPDATE ON configurations
    FOR EACH ROW
    EXECUTE FUNCTION audit_configuration_changes();

-- Initial Configuration Data

-- Insert default system configurations (will be overridden by migration)
INSERT INTO configurations (category, name, value, created_at, updated_at) VALUES
('system', 'rust_log', 'info', NOW(), NOW()),
('system', 'max_connections', '20', NOW(), NOW()),
('system', 'migration_version', '1.0', NOW(), NOW())
ON CONFLICT (category, name) DO NOTHING;

-- Comments for Documentation
COMMENT ON TABLE configurations IS 'Runtime configuration management for topics, RSS feeds, and system settings';
COMMENT ON TABLE articles IS 'Core articles table with JSONB optimization for analysis data';
COMMENT ON TABLE entities IS 'Named entities extracted from articles';
COMMENT ON TABLE article_clusters IS 'Article clustering for related content grouping';

-- Performance Optimization Settings
-- These are recommendations for postgresql.conf

-- Recommended PostgreSQL settings for production:
-- shared_buffers = 256MB (25% of RAM)
-- work_mem = 4MB (per operation)
-- maintenance_work_mem = 64MB (for maintenance operations)
-- max_connections = 100 (based on expected load)
-- wal_buffers = 16MB (WAL buffering)
-- checkpoint_completion_target = 0.9 (smooth checkpoints)
-- random_page_cost = 1.1 (for SSD storage)
-- effective_cache_size = 1GB (available for caching)

-- Schema version for migration tracking
CREATE TABLE schema_migrations (
    version VARCHAR(50) PRIMARY KEY,
    applied_at TIMESTAMPTZ DEFAULT NOW()
);

INSERT INTO schema_migrations (version) VALUES ('postgresql_base_1.0');
