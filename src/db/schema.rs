use tracing::info;

use super::core::Database;
use crate::TARGET_DB;

impl Database {
    pub(crate) async fn initialize_schema(&self) -> Result<(), sqlx::Error> {
        let mut conn = self.pool().acquire().await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL UNIQUE,
                seen_at TEXT NOT NULL,
                pub_date TEXT,
                event_date TEXT,
                is_relevant BOOLEAN NOT NULL,
                category TEXT,
                tiny_summary TEXT,
                analysis TEXT,
                hash TEXT,
                title_domain_hash TEXT,
                r2_url TEXT,
                cluster_id INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
            CREATE INDEX IF NOT EXISTS idx_hash ON articles (hash);
            CREATE INDEX IF NOT EXISTS idx_title_domain_hash ON articles (title_domain_hash);
            CREATE INDEX IF NOT EXISTS idx_r2_url ON articles (r2_url);
            CREATE INDEX IF NOT EXISTS idx_seen_at_r2_url ON articles (seen_at, r2_url);
            CREATE INDEX IF NOT EXISTS idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url);
            CREATE INDEX IF NOT EXISTS idx_articles_event_date ON articles (event_date);
            CREATE INDEX IF NOT EXISTS idx_articles_pub_date ON articles (pub_date);
            CREATE INDEX IF NOT EXISTS idx_articles_cluster_id ON articles (cluster_id);
            
            -- Entity tables for improved article matching
            CREATE TABLE IF NOT EXISTS entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                type TEXT NOT NULL, -- PERSON, ORGANIZATION, LOCATION, EVENT, etc.
                normalized_name TEXT NOT NULL, -- For easier matching
                parent_id INTEGER, -- For hierarchical relations (especially locations)
                UNIQUE(normalized_name, type),
                FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
            );
            CREATE INDEX IF NOT EXISTS idx_entities_normalized_name ON entities (normalized_name);
            CREATE INDEX IF NOT EXISTS idx_entities_type ON entities (type);
            CREATE INDEX IF NOT EXISTS idx_entities_parent_id ON entities (parent_id);
            
            -- Entity-Article relationships
            CREATE TABLE IF NOT EXISTS article_entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_id INTEGER NOT NULL,
                entity_id INTEGER NOT NULL,
                importance TEXT NOT NULL, -- PRIMARY, SECONDARY, MENTIONED
                context TEXT, -- Additional context about the entity in this article
                FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
                FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
                UNIQUE(article_id, entity_id)
            );
            CREATE INDEX IF NOT EXISTS idx_article_entities_article_id ON article_entities (article_id);
            CREATE INDEX IF NOT EXISTS idx_article_entities_entity_id ON article_entities (entity_id);
            CREATE INDEX IF NOT EXISTS idx_article_entities_importance ON article_entities (importance);
            
            -- Article clusters
            CREATE TABLE IF NOT EXISTS article_clusters (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_article_clusters_updated_at ON article_clusters (updated_at);
            
            -- Article-cluster relationships
            CREATE TABLE IF NOT EXISTS article_cluster_members (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_id INTEGER NOT NULL,
                cluster_id INTEGER NOT NULL,
                added_at TIMESTAMP NOT NULL,
                FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
                FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
                UNIQUE(article_id, cluster_id)
            );
            CREATE INDEX IF NOT EXISTS idx_article_cluster_members_article_id ON article_cluster_members (article_id);
            CREATE INDEX IF NOT EXISTS idx_article_cluster_members_cluster_id ON article_cluster_members (cluster_id);
            
            -- Entity alias system tables
            CREATE TABLE IF NOT EXISTS entity_aliases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_id INTEGER,
                canonical_name TEXT NOT NULL,
                alias_text TEXT NOT NULL,
                normalized_canonical TEXT NOT NULL,
                normalized_alias TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                source TEXT NOT NULL, -- STATIC, PATTERN, LLM, ADMIN, etc.
                confidence REAL NOT NULL DEFAULT 1.0,
                created_at TEXT NOT NULL,
                approved_by TEXT,
                approved_at TEXT,
                status TEXT NOT NULL DEFAULT 'APPROVED', -- APPROVED, PENDING, REJECTED
                UNIQUE (normalized_canonical, normalized_alias, entity_type),
                FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE SET NULL
            );
            
            CREATE INDEX IF NOT EXISTS idx_entity_aliases_canonical ON entity_aliases(normalized_canonical, entity_type);
            CREATE INDEX IF NOT EXISTS idx_entity_aliases_alias ON entity_aliases(normalized_alias, entity_type);
            CREATE INDEX IF NOT EXISTS idx_entity_aliases_status ON entity_aliases(status);
            CREATE INDEX IF NOT EXISTS idx_entity_aliases_source ON entity_aliases(source);
            
            -- Negative match table for explicitly rejected pairs
            CREATE TABLE IF NOT EXISTS entity_negative_matches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_id1 INTEGER,
                entity_id2 INTEGER,
                normalized_name1 TEXT NOT NULL,
                normalized_name2 TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                rejected_by TEXT NOT NULL,
                rejected_at TEXT NOT NULL,
                rejection_reason TEXT,
                persistence_level INTEGER NOT NULL DEFAULT 1,
                UNIQUE (normalized_name1, normalized_name2, entity_type),
                FOREIGN KEY (entity_id1) REFERENCES entities (id) ON DELETE SET NULL,
                FOREIGN KEY (entity_id2) REFERENCES entities (id) ON DELETE SET NULL
            );
            
            CREATE INDEX IF NOT EXISTS idx_negative_matches_names ON entity_negative_matches(normalized_name1, normalized_name2);
            CREATE INDEX IF NOT EXISTS idx_negative_matches_type ON entity_negative_matches(entity_type);
            
            -- Alias pattern performance tracking
            CREATE TABLE IF NOT EXISTS alias_pattern_stats (
                pattern_id TEXT PRIMARY KEY,
                pattern_type TEXT NOT NULL, -- REGEX, LLM, TRANSFORMATION
                total_suggestions INTEGER NOT NULL DEFAULT 0,
                approved_count INTEGER NOT NULL DEFAULT 0,
                rejected_count INTEGER NOT NULL DEFAULT 0,
                last_used_at TEXT,
                enabled BOOLEAN NOT NULL DEFAULT TRUE
            );
            
            -- For batch review in admin interface
            CREATE TABLE IF NOT EXISTS alias_review_batches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                admin_id TEXT,
                status TEXT NOT NULL DEFAULT 'OPEN', -- OPEN, COMPLETED
                total_count INTEGER NOT NULL DEFAULT 0,
                processed_count INTEGER NOT NULL DEFAULT 0
            );
            
            CREATE TABLE IF NOT EXISTS alias_review_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                batch_id INTEGER NOT NULL,
                alias_id INTEGER NOT NULL,
                decision TEXT, -- APPROVED, REJECTED, IGNORED
                decided_at TEXT,
                FOREIGN KEY (batch_id) REFERENCES alias_review_batches(id),
                FOREIGN KEY (alias_id) REFERENCES entity_aliases(id)
            );
            
            -- Cache statistics for optimization
            CREATE TABLE IF NOT EXISTS alias_cache_stats (
                normalized_name TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                last_accessed TEXT NOT NULL,
                PRIMARY KEY (normalized_name, entity_type)
            );
        
            CREATE TABLE IF NOT EXISTS rss_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL UNIQUE,
                title TEXT,
                seen_at TEXT NOT NULL,
                pub_date TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url);

            CREATE TABLE IF NOT EXISTS matched_topics_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_text TEXT NOT NULL,
                article_html TEXT NOT NULL,
                article_url TEXT NOT NULL UNIQUE,
                article_title TEXT NOT NULL,
                topic_matched TEXT NOT NULL,
                article_hash TEXT NOT NULL,
                title_domain_hash TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                pub_date TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_matched_topics_article_url ON matched_topics_queue (article_url);

            -- Upgrade to new life_safety_queue table:
            -- ALTER TABLE life_safety_queue RENAME TO life_safety_queue_old;
            CREATE TABLE IF NOT EXISTS life_safety_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_url TEXT NOT NULL UNIQUE,
                article_title TEXT NOT NULL,
                article_text TEXT NOT NULL,
                article_html TEXT NOT NULL,
                article_hash TEXT NOT NULL,
                title_domain_hash TEXT NOT NULL,
                threat TEXT,
                timestamp TEXT NOT NULL,
                pub_date TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_life_safety_article_url ON life_safety_queue (article_url);

            CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL UNIQUE
            );

            CREATE INDEX IF NOT EXISTS idx_devices_device_id ON devices (device_id);
            
            CREATE TABLE IF NOT EXISTS device_subscriptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id INTEGER NOT NULL,
                topic TEXT NOT NULL,
                priority TEXT,
                FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
                UNIQUE(device_id, topic)
            );
            
            CREATE INDEX IF NOT EXISTS idx_topic_device_id ON device_subscriptions (topic, device_id);
            CREATE INDEX IF NOT EXISTS idx_device_subscriptions_device_id_topic ON device_subscriptions (device_id, topic);

            CREATE TABLE IF NOT EXISTS ip_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id INTEGER NOT NULL,
                ip_address TEXT NOT NULL,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
                UNIQUE (device_id, ip_address)
            );
            CREATE INDEX IF NOT EXISTS idx_ip_logs_device_id ON ip_logs (device_id);
            CREATE INDEX IF NOT EXISTS idx_ip_logs_ip_address ON ip_logs (ip_address);
            "#,
        )
        .execute(&mut *conn)
        .await?;
        info!(target: TARGET_DB, "Tables ensured to exist");

        Ok(())
    }
}
