use chrono::{DateTime, TimeZone, Utc};
use rand::Rng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Row, Sqlite,
};
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, instrument};
use url::Url;
use urlnorm::UrlNormalizer;

use crate::{SubscriptionInfo, TARGET_DB};

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Get access to the database pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Gets the article body text content from the analysis JSON field
    pub async fn get_article_text(&self, article_id: i64) -> Result<String, sqlx::Error> {
        let analysis =
            sqlx::query_scalar::<_, Option<String>>("SELECT analysis FROM articles WHERE id = ?")
                .bind(article_id)
                .fetch_one(self.pool())
                .await?;

        if let Some(analysis_json) = analysis {
            // Parse the JSON and extract the article_body field
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&analysis_json) {
                if let Some(body) = parsed.get("article_body").and_then(|v| v.as_str()) {
                    return Ok(body.to_string());
                }
            }
            // If we can't parse JSON or find article_body, return the raw JSON
            return Ok(analysis_json);
        }

        // If no analysis found, try to get tiny_summary
        let tiny_summary = sqlx::query_scalar::<_, Option<String>>(
            "SELECT tiny_summary FROM articles WHERE id = ?",
        )
        .bind(article_id)
        .fetch_one(self.pool())
        .await?;

        if let Some(summary) = tiny_summary {
            return Ok(summary);
        }

        // Last resort - return empty string
        Ok(String::new())
    }

    /// Gets article category and quality score
    pub async fn get_article_metadata(
        &self,
        article_id: i64,
    ) -> Result<(Option<String>, i8), sqlx::Error> {
        // Get category directly from articles table
        let category =
            sqlx::query_scalar::<_, Option<String>>("SELECT category FROM articles WHERE id = ?")
                .bind(article_id)
                .fetch_one(self.pool())
                .await?;

        // Try to get quality from analysis JSON
        let analysis =
            sqlx::query_scalar::<_, Option<String>>("SELECT analysis FROM articles WHERE id = ?")
                .bind(article_id)
                .fetch_one(self.pool())
                .await?;

        let mut quality: i8 = 0;

        if let Some(analysis_json) = analysis {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&analysis_json) {
                if let Some(quality_val) = parsed.get("quality").and_then(|v| v.as_i64()) {
                    quality = quality_val as i8;
                }
            }
        }

        Ok((category, quality))
    }

    /// Gets all entity IDs for an article
    pub async fn get_article_entity_ids(&self, article_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT entity_id FROM article_entities WHERE article_id = ?",
        )
        .bind(article_id)
        .fetch_all(self.pool())
        .await;

        result
    }
}

// Helper method to check if an sqlx error is a database lock error
trait DbLockErrorExt {
    fn is_database_lock_error(&self) -> bool;
}

impl DbLockErrorExt for sqlx::Error {
    fn is_database_lock_error(&self) -> bool {
        match self {
            sqlx::Error::Database(err) => err.code().map_or(false, |c| c == "55P03"), // check if the error is a "lock_timeout" error
            _ => false,
        }
    }
}

impl Database {
    #[instrument(target = "db", level = "info")]
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        info!(target: TARGET_DB, "Creating database pool for: {}", database_url);

        if !Path::new(database_url).exists() {
            return Err(sqlx::Error::Configuration(
                format!("Database file '{}' does not exist", database_url).into(),
            ));
        }

        let connect_options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", database_url))?
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(5))
                .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await?;

        info!(target: TARGET_DB, "Database pool created");

        let mut conn = pool.acquire().await?;
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

        Ok(Database { pool })
    }

    pub async fn instance() -> &'static Database {
        static INSTANCE: OnceCell<Database> = OnceCell::const_new();

        INSTANCE
            .get_or_init(|| async {
                let database_url =
                    std::env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());
                Database::new(&database_url)
                    .await
                    .expect("Failed to initialize database")
            })
            .await
    }

    /// Add a new device to the `devices` table (returns the device ID's internal `id`)
    pub async fn add_device(&self, device_id: &str) -> Result<i64, sqlx::Error> {
        // Attempt to insert the device
        let result = sqlx::query(
            r#"
            INSERT INTO devices (device_id)
            VALUES (?1)
            ON CONFLICT(device_id) DO NOTHING
            RETURNING id;
            "#,
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = result {
            // If the insert was successful, return the newly inserted id
            Ok(row.get("id"))
        } else {
            // If the device already exists, retrieve its id
            let existing_id = sqlx::query_scalar(
                r#"
                SELECT id FROM devices WHERE device_id = ?1;
                "#,
            )
            .bind(device_id)
            .fetch_one(&self.pool)
            .await?;
            Ok(existing_id)
        }
    }

    /// Subscribe a device to a specific topic
    pub async fn subscribe_to_topic(
        &self,
        device_id: &str,
        topic: &str,
        priority: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let device_id_internal = self.add_device(device_id).await?;
        sqlx::query(
            r#"
            INSERT INTO device_subscriptions (device_id, topic, priority)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(device_id, topic) DO UPDATE SET priority = ?3
            "#,
        )
        .bind(device_id_internal)
        .bind(topic)
        .bind(priority)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Unsubscribe a device from a specific topic
    pub async fn unsubscribe_from_topic(
        &self,
        device_id: &str,
        topic: &str,
    ) -> Result<bool, sqlx::Error> {
        let rows_affected = sqlx::query(
            r#"
            DELETE FROM device_subscriptions
            WHERE device_id = (SELECT id FROM devices WHERE device_id = ?1)
            AND topic = ?2;
            "#,
        )
        .bind(device_id)
        .bind(topic)
        .execute(&self.pool)
        .await?
        .rows_affected();

        // Return true if a subscription was removed, false otherwise
        Ok(rows_affected > 0)
    }

    /// Remove a device token and its subscriptions from the database
    pub async fn remove_device_token(&self, device_token: &str) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        // Delete subscriptions for the device
        sqlx::query(
            r#"
                DELETE FROM device_subscriptions
                WHERE device_id = (SELECT id FROM devices WHERE device_id = ?1);
                "#,
        )
        .bind(device_token)
        .execute(&mut *transaction)
        .await?;

        // Delete the device itself
        sqlx::query(
            r#"
                DELETE FROM devices
                WHERE device_id = ?1;
                "#,
        )
        .bind(device_token)
        .execute(&mut *transaction)
        .await?;

        // Commit the transaction
        transaction.commit().await?;
        Ok(())
    }

    /// Fetch all device IDs subscribed to a specific topic with high priority
    pub async fn fetch_devices_for_topic(
        &self,
        topic: &str,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT d.device_id, COALESCE(ds.priority, 'low') as priority
            FROM device_subscriptions ds
            JOIN devices d ON ds.device_id = d.id
            WHERE ds.topic = ?1 AND (ds.priority = 'high' OR ds.priority IS NULL AND 'high' = 'high');
            "#,
        )
        .bind(topic)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get("device_id"), row.get("priority")))
            .collect())
    }

    #[instrument(target = "db", level = "info", skip(self, url, title))]
    pub async fn add_to_queue(
        &self,
        url: &str,
        title: Option<&str>,
        pub_date: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        if url.trim().is_empty() {
            error!(target: TARGET_DB, "Attempted to add an empty URL to the queue");
            return Err(sqlx::Error::Protocol("Empty URL provided".into()));
        }

        // 1) Parse the URL
        let parsed_url = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(target: TARGET_DB, "Attempted to add an invalid URL ({}) to the queue: {}", url, e);
                return Err(sqlx::Error::Protocol("Invalid URL provided".into()));
            }
        };

        // 2) Normalize the URL
        let normalizer = UrlNormalizer::default();
        let normalized_url = normalizer.compute_normalization_string(&parsed_url);

        // 3) Check existence in articles table
        let exists_in_articles = sqlx::query("SELECT 1 FROM articles WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_articles {
            debug!(target: TARGET_DB, "URL already exists in articles: {}", normalized_url);
            return Ok(false);
        }

        // 4) Check existence in rss_queue
        let exists_in_queue = sqlx::query("SELECT 1 FROM rss_queue WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_queue {
            debug!(target: TARGET_DB, "URL already exists in the queue: {}", &normalized_url);
            return Ok(false);
        }

        // 5) Insert into rss_queue with pub_date
        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        debug!(target: TARGET_DB, "Adding URL to queue: {}", normalized_url);
        sqlx::query(
            r#"
        INSERT INTO rss_queue (url, normalized_url, title, seen_at, pub_date)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(normalized_url) DO NOTHING
        "#,
        )
        .bind(url)
        .bind(&normalized_url)
        .bind(title)
        .bind(seen_at)
        .bind(pub_date) // <--- store the pub_date here
        .execute(&self.pool)
        .await?;

        debug!(target: TARGET_DB, "URL added to queue: {}", normalized_url);
        Ok(true)
    }

    /// Add an entry to the matched topics queue
    #[instrument(
        target = "db",
        level = "info",
        skip(self, article_text, article_html, article_url, topic_matched)
    )]
    pub async fn add_to_matched_topics_queue(
        &self,
        article_text: &str,
        article_html: &str,
        article_url: &str,
        article_title: &str,
        article_hash: &str,
        title_domain_hash: &str,
        topic_matched: &str,
        pub_date: Option<&str>, // <-- new parameter
    ) -> Result<(), sqlx::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        let result = sqlx::query(
            r#"
            INSERT INTO matched_topics_queue (
                article_text, article_html, article_url, article_title,
                article_hash, title_domain_hash, topic_matched, timestamp, pub_date
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(article_url) DO NOTHING
            "#,
        )
        .bind(article_text)
        .bind(article_html)
        .bind(article_url)
        .bind(article_title)
        .bind(article_hash)
        .bind(title_domain_hash)
        .bind(topic_matched)
        .bind(timestamp)
        .bind(pub_date) // <-- store pub_date
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                debug!(target: TARGET_DB, "Successfully added to matched topics queue: article_url={}, topic_matched={}", article_url, topic_matched);
            }
            Err(sqlx::Error::Database(db_err))
                if db_err.message().contains("UNIQUE constraint failed") =>
            {
                debug!(target: TARGET_DB, "Duplicate article_url detected, skipping insert: {}", article_url);
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to add to matched topics queue: {:?}", e);
                return Err(e);
            }
        }

        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn add_to_life_safety_queue(
        &self,
        threat: &str,
        article_url: &str,
        article_title: &str,
        article_text: &str,
        article_html: &str,
        article_hash: &str,
        title_domain_hash: &str,
        pub_date: Option<&str>, // <-- new parameter
    ) -> Result<(), sqlx::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        let result = sqlx::query(
            r#"
            INSERT INTO life_safety_queue (
                article_url, article_title, article_text, article_html,
                article_hash, title_domain_hash, threat, timestamp, pub_date
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(article_url) DO NOTHING
            "#,
        )
        .bind(article_url)
        .bind(article_title)
        .bind(article_text)
        .bind(article_html)
        .bind(article_hash)
        .bind(title_domain_hash)
        .bind(threat)
        .bind(timestamp)
        .bind(pub_date) // <-- store pub_date
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                debug!(target: TARGET_DB, "Successfully added to life safety queue: {}", article_url);
            }
            Err(sqlx::Error::Database(db_err))
                if db_err.message().contains("UNIQUE constraint failed") =>
            {
                debug!(target: TARGET_DB, "Duplicate article_url detected, skipping insert: {}", article_url);
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to add to life safety queue: {:?}", e);
                return Err(e);
            }
        }

        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_from_life_safety_queue(
        &self,
    ) -> Result<
        Option<(
            String,         // article_url
            String,         // article_title
            String,         // article_text
            String,         // article_html
            String,         // article_hash
            String,         // title_domain_hash
            String,         // threat
            Option<String>, // pub_date
        )>,
        sqlx::Error,
    > {
        debug!(target: TARGET_DB, "Fetching and deleting item from life safety queue");

        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT 
                id,
                article_url,
                article_title,
                article_text,
                article_html,
                article_hash,
                title_domain_hash,
                threat,
                pub_date
            FROM life_safety_queue
            ORDER BY timestamp ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(row) = row {
            let id: i64 = row.get("id");
            let article_url: String = row.get("article_url");
            let article_title: String = row.get("article_title");
            let article_text: String = row.get("article_text");
            let article_html: String = row.get("article_html");
            let article_hash: String = row.get("article_hash");
            let title_domain_hash: String = row.get("title_domain_hash");
            let threat: String = row.get("threat");
            let pub_date: Option<String> = row.get("pub_date"); // <-- retrieve pub_date

            sqlx::query("DELETE FROM life_safety_queue WHERE id = ?1")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            debug!(target: TARGET_DB, "Fetched and deleted item from life safety queue: {}", article_url);

            Ok(Some((
                article_url,
                article_title,
                article_text,
                article_html,
                article_hash,
                title_domain_hash,
                threat,
                pub_date,
            )))
        } else {
            debug!(target: TARGET_DB, "No new items found in life safety queue");
            transaction.rollback().await?;
            Ok(None)
        }
    }

    #[instrument(target = "db", level = "info", skip(self, url, category, analysis))]
    pub async fn add_article(
        &self,
        url: &str,
        is_relevant: bool,
        category: Option<&str>,
        analysis: Option<&str>,
        tiny_summary: Option<&str>,
        hash: Option<&str>,
        title_domain_hash: Option<&str>,
        r2_url: Option<&str>,
        pub_date: Option<&str>,
        event_date: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        // Parse the URL
        let parsed_url = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(target: TARGET_DB, "Attempted to add an invalid URL ({}) to the queue: {}", url, e);
                return Err(sqlx::Error::Protocol("Invalid URL provided".into()));
            }
        };
        // Normalize the URL
        let normalizer = UrlNormalizer::default();
        let normalized_url = normalizer.compute_normalization_string(&parsed_url);
        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();
        debug!(target: TARGET_DB, "Adding/updating article: {}", url);

        let mut backoff = 100; // initial delay in milliseconds
        let max_retries = 5;

        for attempt in 1..=max_retries {
            match sqlx::query_as::<_, (i64,)>(
            r#"
            INSERT INTO articles (url, normalized_url, seen_at, pub_date, event_date, is_relevant, category, analysis, tiny_summary, hash, title_domain_hash, r2_url)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(url) DO UPDATE SET
                url = excluded.url,
                seen_at = excluded.seen_at,
                pub_date = excluded.pub_date,
                event_date = excluded.event_date,
                is_relevant = excluded.is_relevant,
                category = excluded.category,
                analysis = excluded.analysis,
                tiny_summary = excluded.tiny_summary,
                hash = excluded.hash,
                title_domain_hash = excluded.title_domain_hash,
                r2_url = excluded.r2_url
            RETURNING id
            "#,
        )
        .bind(url)
        .bind(&normalized_url)
        .bind(&seen_at)
        .bind(&pub_date)
        .bind(&event_date)
        .bind(is_relevant)
        .bind(category)
        .bind(analysis)
        .bind(tiny_summary)
        .bind(hash)
        .bind(title_domain_hash)
        .bind(r2_url)
        .fetch_one(&self.pool)
        .await {
            Ok((id,)) => {
                debug!(target: TARGET_DB, "Article added/updated: {} with id {}", url, id);
                return Ok(id);
            }
            Err(err) => {
                if err.is_database_lock_error() {
                    info!(target: TARGET_DB, "Database is locked, waiting {}ms before retrying attempt {}/{}: {}", backoff, attempt, max_retries, url);
                    sleep(Duration::from_millis(backoff)).await;
                    backoff = backoff.saturating_mul(2); // exponential backoff
                    if attempt == max_retries {
                        // Introduce some randomness to avoid the "thundering herd problem"
                        let random_jitter = rand::rng().random_range(0..200);
                        backoff += random_jitter;
                        sleep(Duration::from_millis(backoff)).await;
                    }
                } else {
                    error!(target: TARGET_DB, "Failed to add article: {}", err);
                    return Err(err);
                }
            }
        }
        }

        Err(sqlx::Error::Protocol(
            "Maximum retries exceeded for adding article".into(),
        ))
    }

    /// Update an article with R2 details
    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn update_article_with_r2_details(
        &self,
        url: &str,
        r2_url: &str,
    ) -> Result<(), sqlx::Error> {
        debug!(target: TARGET_DB, "Updating article with R2 details: {}", url);

        let result = sqlx::query(
            r#"
            UPDATE articles
            SET r2_url = ?1
            WHERE url = ?2
            "#,
        )
        .bind(r2_url)
        .bind(url)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                debug!(target: TARGET_DB, "Successfully updated R2 details for article: {}", url);
                Ok(())
            }
            Err(err) => {
                error!(target: TARGET_DB, "Failed to update R2 details for article: {}: {:?}", url, err);
                Err(err)
            }
        }
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn has_seen(&self, url: &str) -> Result<bool, sqlx::Error> {
        debug!(target: TARGET_DB, "Checking if article has been seen: {}", url);

        // Parse the URL
        let parsed_url = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(target: TARGET_DB, "Attempted to add an invalid URL ({}) to the queue: {}", url, e);
                return Err(sqlx::Error::Protocol("Invalid URL provided".into()));
            }
        };

        // Normalize the URL
        let normalizer = UrlNormalizer::default();
        let normalized_url = normalizer.compute_normalization_string(&parsed_url);

        let row = sqlx::query("SELECT 1 FROM articles WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(&self.pool)
            .await?;

        let seen = row.is_some();
        debug!(target: TARGET_DB, "Article seen status for {}: {}", normalized_url, seen);
        Ok(seen)
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_url_from_rss_queue(
        &self,
        order: &str,
    ) -> Result<Option<(String, Option<String>, Option<String>)>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        // Grab `pub_date` in the SELECT
        let row = match order {
            "oldest" => {
                sqlx::query(
                    r#"
                    SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title, rss_queue.pub_date
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY rss_queue.seen_at ASC
                    LIMIT 1
                    "#
                )
                .fetch_optional(&mut *transaction)
                .await?
            },
            "newest" => {
                sqlx::query(
                    r#"
                    SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title, rss_queue.pub_date
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY rss_queue.seen_at DESC
                    LIMIT 1
                    "#
                )
                .fetch_optional(&mut *transaction)
                .await?
            },
            _ => {
                sqlx::query(
                    r#"
                    SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title, rss_queue.pub_date
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY RANDOM()
                    LIMIT 1
                    "#
                )
                .fetch_optional(&mut *transaction)
                .await?
            }
        };

        if let Some(row) = row {
            let url: String = row.get("url");
            let normalized_url: String = row.get("normalized_url");
            let title: Option<String> = row.get("title");
            let pub_date: Option<String> = row.get("pub_date"); // <-- retrieve it
            sqlx::query("DELETE FROM rss_queue WHERE normalized_url = ?1")
                .bind(&normalized_url)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            Ok(Some((url, title, pub_date)))
        } else {
            transaction.rollback().await?;
            Ok(None)
        }
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_from_matched_topics_queue(
        &self,
    ) -> Result<
        Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        debug!(target: TARGET_DB, "Fetching and deleting item from matched topics queue");

        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT 
                id,
                article_text,
                article_html,
                article_url,
                article_title,
                article_hash,
                title_domain_hash,
                topic_matched,
                pub_date
            FROM matched_topics_queue
            ORDER BY timestamp ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(row) = row {
            let id: i64 = row.get("id");
            let article_text: String = row.get("article_text");
            let article_html: String = row.get("article_html");
            let article_url: String = row.get("article_url");
            let article_title: String = row.get("article_title");
            let article_hash: String = row.get("article_hash");
            let title_domain_hash: String = row.get("title_domain_hash");
            let topic_matched: String = row.get("topic_matched");
            let pub_date: Option<String> = row.get("pub_date"); // <-- retrieve pub_date

            sqlx::query("DELETE FROM matched_topics_queue WHERE id = ?1")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            debug!(target: TARGET_DB, "Fetched and deleted item from matched topics queue: {}", article_url);

            Ok(Some((
                article_text,
                article_html,
                article_url,
                article_title,
                article_hash,
                title_domain_hash,
                topic_matched,
                pub_date,
            )))
        } else {
            debug!(target: TARGET_DB, "No new items found in matched topics queue");
            transaction.rollback().await?;
            Ok(None)
        }
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn count_queue_entries(&self) -> Result<i64, sqlx::Error> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM rss_queue")
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.get("count");
        debug!(target: TARGET_DB, "Counted {} entries in the queue", count);
        Ok(count)
    }

    /// Cleans up the rss_queue by removing URLs already present in the articles table.
    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn clean_queue(&self) -> Result<u64, sqlx::Error> {
        debug!(target: TARGET_DB, "Cleaning up the queue by removing processed URLs");

        let affected_rows = sqlx::query(
            r#"
                DELETE FROM rss_queue
                WHERE normalized_url IN (SELECT normalized_url FROM articles)
                "#,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        info!(target: TARGET_DB, "Cleaned up {} entries from the queue", affected_rows);
        Ok(affected_rows)
    }

    // Check if the hash of the text has already been seen, to filter out articles that
    // have multiple URLs for the same identical text.
    pub async fn has_hash(&self, hash: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 FROM articles WHERE hash = ?1")
            .bind(hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    // Check if the hash of the domain and title has already been seen, another way to
    // filter out articles that have multiple URLs for the same article.
    pub async fn has_title_domain_hash(
        &self,
        title_domain_hash: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 FROM articles WHERE title_domain_hash = ?1")
            .bind(title_domain_hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    /// Collect statistics from various tables in the database
    pub async fn collect_stats(&self) -> Result<String, sqlx::Error> {
        let queries = vec![
            "SELECT COUNT(*) FROM articles WHERE is_relevant = false;",
            "SELECT COUNT(*) FROM articles WHERE is_relevant = true;",
            "SELECT COUNT(*) FROM rss_queue;",
            "SELECT COUNT(*) FROM life_safety_queue;",
            "SELECT COUNT(*) FROM matched_topics_queue;",
            "SELECT COUNT(*) FROM devices;",
        ];

        let mut results = vec![];
        for query in queries {
            let count: i64 = sqlx::query_scalar(query).fetch_one(&self.pool).await?;
            results.push(count);
        }

        Ok(results
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(":"))
    }

    pub async fn fetch_unseen_articles(
        &self,
        device_id: &str,
        seen_articles: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        // First, get the topics the device is subscribed to
        let subscribed_topics = sqlx::query_as::<_, (String,)>(
            "SELECT topic FROM device_subscriptions ds
             JOIN devices d ON ds.device_id = d.id
             WHERE d.device_id = ?",
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.0)
        .collect::<Vec<String>>();

        let include_alerts = subscribed_topics
            .iter()
            .any(|topic| topic.starts_with("Alert"));

        let topic_placeholders = subscribed_topics
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let category_condition = if include_alerts {
            format!("(category IN ({}) OR category IS NULL)", topic_placeholders)
        } else {
            format!("category IN ({})", topic_placeholders)
        };

        let query = if seen_articles.is_empty() {
            format!(
                "SELECT r2_url
                 FROM articles
                 WHERE r2_url IS NOT NULL
                 AND datetime(seen_at, 'unixepoch') > datetime('now', '-12 hours')
                 AND {}
                 AND is_relevant = 1;",
                category_condition
            )
        } else {
            let seen_placeholders = seen_articles
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT r2_url
                 FROM articles
                 WHERE r2_url IS NOT NULL
                 AND r2_url NOT IN ({})
                 AND datetime(seen_at, 'unixepoch') > datetime('now', '-12 hours')
                 AND {}
                 AND is_relevant = 1;",
                seen_placeholders, category_condition
            )
        };

        info!("Generated SQL query: {}", query);

        let mut query_builder = sqlx::query(&query);

        // Bind seen articles if there are any
        for article in seen_articles {
            query_builder = query_builder.bind(article);
        }

        // Bind subscribed topics
        for topic in &subscribed_topics {
            query_builder = query_builder.bind(topic);
        }

        info!(
            "Executing query to fetch unseen articles for device_id: {}",
            device_id
        );
        let rows = query_builder.fetch_all(&self.pool).await?;
        let unseen_articles: Vec<String> = rows.into_iter().map(|row| row.get("r2_url")).collect();

        info!("Fetched unseen articles: {:?}", unseen_articles);
        if unseen_articles.is_empty() {
            info!("No unseen articles found for the given list of seen articles and device_id.");
        }

        Ok(unseen_articles)
    }

    pub async fn log_ip_address(
        &self,
        device_id: &str,
        ip_address: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp(); // This gives us the Unix timestamp
        sqlx::query(
            r#"
            INSERT INTO ip_logs (device_id, ip_address, first_seen, last_seen)
            VALUES (
                (SELECT id FROM devices WHERE device_id = ?1),
                ?2,
                ?3,
                ?3
            )
            ON CONFLICT (device_id, ip_address) DO UPDATE SET
                last_seen = excluded.last_seen
            "#,
        )
        .bind(device_id)
        .bind(ip_address)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_ip_logs_for_device(
        &self,
        device_id: &str,
    ) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i64, i64)>(
            r#"
            SELECT ip_address, first_seen, last_seen
            FROM ip_logs
            JOIN devices ON ip_logs.device_id = devices.id
            WHERE devices.device_id = ?1
            ORDER BY last_seen DESC
            "#,
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(ip, first, last)| {
                (
                    ip,
                    match Utc.timestamp_opt(first, 0) {
                        chrono::LocalResult::Single(dt) => dt,
                        _ => Utc::now(), // Fallback to current time if timestamp is ambiguous or out of range
                    },
                    match Utc.timestamp_opt(last, 0) {
                        chrono::LocalResult::Single(dt) => dt,
                        _ => Utc::now(), // Fallback to current time if timestamp is ambiguous or out of range
                    },
                )
            })
            .collect())
    }

    pub async fn get_device_subscriptions(
        &self,
        device_id: &str,
    ) -> Result<Vec<SubscriptionInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT ds.topic, COALESCE(ds.priority, 'low') as priority
            FROM device_subscriptions ds
            JOIN devices d ON ds.device_id = d.id
            WHERE d.device_id = ?1
            ORDER BY ds.topic;
            "#,
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(topic, priority)| SubscriptionInfo { topic, priority })
            .collect())
    }

    pub async fn get_article_details_by_id(
        &self,
        article_id: i64,
    ) -> Result<Option<(String, Option<String>, String)>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT r2_url, tiny_summary, analysis
            FROM articles
            WHERE id = ?1
            "#,
        )
        .bind(article_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let json_url: String = row.get("r2_url");
            let tiny_summary: Option<String> = row.get("tiny_summary");
            let analysis_json: String = row.get("analysis");

            // Extract title from the analysis JSON
            let tiny_title =
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&analysis_json) {
                    json["tiny_title"].as_str().map(|s| s.to_string())
                } else {
                    None
                };

            Ok(Some((
                json_url,
                tiny_title,
                tiny_summary.unwrap_or_default(),
            )))
        } else {
            Ok(None)
        }
    }

    /// Get article details with dates (for temporal matching)
    pub async fn get_article_details_with_dates(
        &self,
        article_id: i64,
    ) -> Result<(Option<String>, Option<String>), sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT pub_date, event_date
            FROM articles
            WHERE id = ?1
            "#,
        )
        .bind(article_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let pub_date: Option<String> = row.get("pub_date");
            let event_date: Option<String> = row.get("event_date");

            Ok((pub_date, event_date))
        } else {
            Ok((None, None))
        }
    }

    // ----- Entity Management Functions -----

    /// Add a new entity to the database or return existing entity ID if it already exists
    pub async fn add_entity(
        &self,
        name: &str,
        entity_type: &str,
        normalized_name: &str,
        parent_id: Option<i64>,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO entities (name, type, normalized_name, parent_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(normalized_name, type) DO UPDATE SET
                name = excluded.name,
                parent_id = excluded.parent_id
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(entity_type)
        .bind(normalized_name)
        .bind(parent_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.get("id"))
    }

    /// Link an entity to an article with specified importance
    pub async fn add_entity_to_article(
        &self,
        article_id: i64,
        entity_id: i64,
        importance: &str,
        context: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO article_entities (article_id, entity_id, importance, context)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(article_id, entity_id) DO UPDATE SET
                importance = excluded.importance,
                context = excluded.context
            "#,
        )
        .bind(article_id)
        .bind(entity_id)
        .bind(importance)
        .bind(context)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all entities for a specific article
    pub async fn get_article_entities(
        &self,
        article_id: i64,
    ) -> Result<Vec<(i64, String, String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT e.id, e.name, e.type, ae.importance
            FROM entities e
            JOIN article_entities ae ON e.id = ae.entity_id
            WHERE ae.article_id = ?1
            ORDER BY ae.importance, e.type, e.name
            "#,
        )
        .bind(article_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("name"),
                    row.get("type"),
                    row.get("importance"),
                )
            })
            .collect())
    }

    /// Get detail information about a specific entity
    pub async fn get_entity_details(
        &self,
        entity_id: i64,
    ) -> Result<Option<(String, String, Option<i64>)>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT name, type, parent_id
            FROM entities
            WHERE id = ?1
            "#,
        )
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some((
                row.get("name"),
                row.get("type"),
                row.get("parent_id"),
            )))
        } else {
            Ok(None)
        }
    }

    /// Get articles that share significant entities with the given entity IDs
    pub async fn get_articles_by_entities(
        &self,
        entity_ids: &[i64],
        limit: u64,
    ) -> Result<Vec<(i64, Option<String>, Option<String>, i64, i64)>, sqlx::Error> {
        // Convert entity_ids to a JSON array string for SQLite's json_each function
        let entity_ids_json = serde_json::to_string(entity_ids).map_err(|e| {
            sqlx::Error::Protocol(format!("JSON serialization error: {}", e).into())
        })?;

        // Query for articles that share entities, prioritizing those with PRIMARY importance
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.pub_date as published_date, a.category, a.quality_score,
                   COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                   COUNT(ae.entity_id) as total_count
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
        )
        .bind(&entity_ids_json)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        // Convert rows to tuples - must match the expected return type in function signature
        let results: Vec<(i64, Option<String>, Option<String>, i64, i64)> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get::<i64, _>("primary_count"),
                    row.get::<i64, _>("total_count"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Get articles that share significant entities with the given entity IDs
    /// and are within a date window around the source article date
    pub async fn get_articles_by_entities_with_date(
        &self,
        entity_ids: &[i64],
        limit: u64,
        source_date: &str,
    ) -> Result<Vec<(i64, Option<String>, Option<String>, i64, i64)>, sqlx::Error> {
        // Log the search criteria
        info!(target: TARGET_DB, "Looking for articles with entities: {:?}, source date: {}, limit: {}", 
              entity_ids, source_date, limit);

        // Convert entity_ids to a JSON array string for SQLite's json_each function
        let entity_ids_json = serde_json::to_string(entity_ids).map_err(|e| {
            sqlx::Error::Protocol(format!("JSON serialization error: {}", e).into())
        })?;

        // First, try a query WITHOUT the date filter to see if we have ANY matching articles
        let check_query = r#"
            SELECT COUNT(*) 
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
            "#;

        let total_matching: i64 = match sqlx::query_scalar(check_query)
            .bind(&entity_ids_json)
            .fetch_one(&self.pool)
            .await
        {
            Ok(count) => {
                info!(target: TARGET_DB, "Found {} total articles that share entities with: {:?} (without date filter)", 
                    count, entity_ids);
                count
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to check total matching articles: {}", e);
                0
            }
        };

        // Base query without date filtering
        let base_query = r#"
            SELECT a.id, a.pub_date as published_date, a.category,
                   COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                   COUNT(ae.entity_id) as total_count
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
        "#;

        // Query with date filtering using COALESCE to check both event_date and pub_date
        // and using today's date as the reference point for the window
        let query_with_date_filter = format!(
            r#"
            {}
            AND COALESCE(
                date(substr(a.event_date, 1, 10)),
                date(substr(a.pub_date, 1, 10))
            ) BETWEEN date('now', '-14 days')
                   AND date('now', '+1 day')
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
            base_query
        );

        // Query without date filtering
        let query_without_date_filter = format!(
            r#"
            {}
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
            base_query
        );

        // Log the date window we're using
        info!(target: TARGET_DB, "Using date window: from today - 14 days to today + 1 day");

        // Decide which query to use and execute it
        let rows = if !source_date.is_empty() {
            info!(target: TARGET_DB, "Using query with date filtering");
            sqlx::query(&query_with_date_filter)
                .bind(&entity_ids_json)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
        } else {
            info!(target: TARGET_DB, "No source date provided, skipping date filtering");
            sqlx::query(&query_without_date_filter)
                .bind(&entity_ids_json)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
        };

        info!(target: TARGET_DB, "Entity search returned {} results for entities: {:?} using date window", 
            rows.len(), entity_ids);

        // If we got no results with date filter but had matches without it, log this critical info
        if rows.is_empty() && total_matching > 0 && !source_date.is_empty() {
            error!(target: TARGET_DB,
                "CRITICAL: Date filter is eliminating all potential matches! Found {} matching articles but 0 after date filter", 
                total_matching);

            // Let's log a few of the matching articles that were filtered out
            let sample_query = r#"
                SELECT a.id, a.pub_date, a.event_date, a.category,
                       COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                       COUNT(ae.entity_id) as total_count
                FROM articles a
                JOIN article_entities ae ON a.id = ae.article_id
                WHERE ae.entity_id IN (SELECT value FROM json_each(?))
                GROUP BY a.id
                ORDER BY a.pub_date DESC
                LIMIT 5
                "#;

            match sqlx::query(sample_query)
                .bind(&entity_ids_json)
                .fetch_all(&self.pool)
                .await
            {
                Ok(samples) => {
                    for row in samples {
                        let id: i64 = row.get("id");
                        let pub_date: Option<String> = row.get("pub_date");
                        let event_date: Option<String> = row.get("event_date");
                        let category: Option<String> = row.get("category");
                        let primary_count: i64 = row.get("primary_count");
                        let total_count: i64 = row.get("total_count");

                        info!(target: TARGET_DB,
                            "Example match filtered by date: article_id={}, pub_date={}, event_date={}, category={}, primary_count={}, total_count={}",
                            id,
                            pub_date.unwrap_or_default(),
                            event_date.unwrap_or_default(),
                            category.unwrap_or_default(),
                            primary_count,
                            total_count);
                    }
                }
                Err(e) => {
                    error!(target: TARGET_DB, "Failed to get sample filtered matches: {}", e);
                }
            }
        }

        // Convert rows to tuples - must match the expected return type (5-tuple)
        let results: Vec<(i64, Option<String>, Option<String>, i64, i64)> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get::<i64, _>("primary_count"),
                    row.get::<i64, _>("total_count"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Process entity extraction JSON from LLM and add entities to an article
    pub async fn process_entity_extraction(
        &self,
        article_id: i64,
        extraction_json: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let entities: serde_json::Value = serde_json::from_str(extraction_json)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid JSON: {}", e).into()))?;

        let mut added_entity_ids = Vec::new();

        if let Some(entities_array) = entities["entities"].as_array() {
            for entity in entities_array {
                // Extract entity data
                let name = entity["name"]
                    .as_str()
                    .ok_or_else(|| sqlx::Error::Protocol("Missing entity name".into()))?;
                let entity_type = entity["entity_type"]
                    .as_str()
                    .ok_or_else(|| sqlx::Error::Protocol("Missing entity type".into()))?;
                // Create a separate variable for the lowercase name to extend its lifetime
                let lowercase_name = name.to_lowercase();
                let normalized_name = entity["normalized_name"]
                    .as_str()
                    .unwrap_or(lowercase_name.as_str());
                let importance = entity["importance"].as_str().unwrap_or("MENTIONED");

                // Add entity to database
                let entity_id = self
                    .add_entity(name, entity_type, normalized_name, None)
                    .await?;

                // Add entity to article with importance
                self.add_entity_to_article(article_id, entity_id, importance, None)
                    .await?;

                added_entity_ids.push(entity_id);
            }
        }

        // If event_date is present in the extraction, update the article
        if let Some(event_date) = entities["event_date"].as_str() {
            if !event_date.is_empty() {
                sqlx::query(
                    r#"
                    UPDATE articles
                    SET event_date = ?1
                    WHERE id = ?2
                    "#,
                )
                .bind(event_date)
                .bind(article_id)
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(added_entity_ids)
    }

    /// Find articles that have associated entities and were published after a certain date
    pub async fn find_articles_with_entities(
        &self,
        date_threshold: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query_scalar(
            r#"
            SELECT DISTINCT a.id
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE (a.pub_date >= ?1 OR a.pub_date IS NULL)
            ORDER BY a.id DESC
            LIMIT 5000
            "#,
        )
        .bind(date_threshold)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // ----- Entity Alias System Functions -----

    /// Add a new entity alias to the database
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id, canonical_name, alias_text, entity_type)
    )]
    pub async fn add_entity_alias(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        alias_text: &str,
        entity_type: &str,
        source: &str,
        confidence: f64,
        status: Option<&str>,
        approved_by: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        // Normalize both names for consistent matching
        let normalizer = crate::entity::normalizer::EntityNormalizer::new();
        let entity_type_enum = crate::entity::types::EntityType::from_str(entity_type)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid entity type: {}", e).into()))?;

        let normalized_canonical = normalizer.normalize(canonical_name, entity_type_enum);
        let normalized_alias = normalizer.normalize(alias_text, entity_type_enum);

        // If the normalized forms are identical, skip adding this alias
        if normalized_canonical == normalized_alias {
            debug!(target: TARGET_DB,
                "Skipping alias with identical normalized form: {} ↔ {} ({}) = {} = {}",
                canonical_name, alias_text, entity_type, normalized_canonical, normalized_alias
            );

            // Return a "dummy" ID of 0 to indicate nothing was added but operation succeeded
            return Ok(0);
        }

        // Check for negative matches
        let is_negative_match = self
            .is_negative_match(&normalized_canonical, &normalized_alias, entity_type)
            .await?;
        if is_negative_match {
            debug!(target: TARGET_DB,
                "Skipping alias due to negative match: {} ↔ {} ({})",
                canonical_name, alias_text, entity_type
            );
            return Ok(0);
        }

        let created_at = chrono::Utc::now().to_rfc3339();
        let status = status.unwrap_or("PENDING");

        // Get the approved_at timestamp if status is APPROVED and we have an approver
        let approved_at = if status == "APPROVED" && approved_by.is_some() {
            Some(created_at.clone())
        } else {
            None
        };

        let result = sqlx::query(
            r#"
            INSERT INTO entity_aliases (
                entity_id, canonical_name, alias_text, normalized_canonical, normalized_alias, 
                entity_type, source, confidence, created_at, approved_by, approved_at, status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(normalized_canonical, normalized_alias, entity_type) DO UPDATE SET
                entity_id = COALESCE(excluded.entity_id, entity_aliases.entity_id),
                source = excluded.source,
                confidence = MAX(entity_aliases.confidence, excluded.confidence),
                status = CASE 
                    WHEN excluded.status = 'APPROVED' OR entity_aliases.status = 'APPROVED' THEN 'APPROVED'
                    WHEN excluded.status = 'REJECTED' OR entity_aliases.status = 'REJECTED' THEN 'REJECTED'
                    ELSE excluded.status
                END,
                approved_by = COALESCE(excluded.approved_by, entity_aliases.approved_by),
                approved_at = COALESCE(excluded.approved_at, entity_aliases.approved_at)
            RETURNING id
            "#,
        )
        .bind(entity_id)
        .bind(canonical_name)
        .bind(alias_text)
        .bind(normalized_canonical)
        .bind(normalized_alias)
        .bind(entity_type)
        .bind(source)
        .bind(confidence)
        .bind(created_at)
        .bind(approved_by)
        .bind(approved_at)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");
        info!(target: TARGET_DB,
            "Added/updated entity alias: {} ↔ {} ({}) [id={}]",
            canonical_name, alias_text, entity_type, id
        );

        Ok(id)
    }

    /// Add multiple aliases with the same canonical name
    #[instrument(
        target = "db",
        level = "info",
        skip(self, canonical_name, aliases, entity_type)
    )]
    pub async fn add_multiple_aliases(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        aliases: &[&str],
        entity_type: &str,
        source: &str,
        confidence: f64,
        status: Option<&str>,
        approved_by: Option<&str>,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let mut ids = Vec::new();

        for alias in aliases {
            let id = self
                .add_entity_alias(
                    entity_id,
                    canonical_name,
                    alias,
                    entity_type,
                    source,
                    confidence,
                    status,
                    approved_by,
                )
                .await?;

            if id > 0 {
                ids.push(id);
            }
        }

        Ok(ids)
    }

    /// Add a manually created alias by an admin user
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id, canonical_name, alias_text, entity_type)
    )]
    pub async fn add_admin_alias(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        alias_text: &str,
        entity_type: crate::entity::types::EntityType,
        admin_id: &str,
        notes: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        // Look up entity ID if not provided but we have a canonical name
        let entity_id = if entity_id.is_none() {
            let normalizer = crate::entity::normalizer::EntityNormalizer::new();
            let normalized_name = normalizer.normalize(canonical_name, entity_type);

            // Try to find the entity
            let entity_row = sqlx::query(
                r#"
                SELECT id FROM entities
                WHERE normalized_name = ? AND type = ?
                "#,
            )
            .bind(&normalized_name)
            .bind(entity_type.to_string())
            .fetch_optional(&self.pool)
            .await?;

            entity_row.map(|row| row.get("id"))
        } else {
            entity_id
        };

        // Add the alias with ADMIN source and APPROVED status
        let source = format!("ADMIN:{}", notes.unwrap_or("manual"));
        self.add_entity_alias(
            entity_id,
            canonical_name,
            alias_text,
            &entity_type.to_string(),
            &source,
            1.0, // Admin aliases get maximum confidence
            Some("APPROVED"),
            Some(admin_id),
        )
        .await
    }

    /// Check if two entity names are the same according to the alias system
    #[instrument(target = "db", level = "info", skip(self, name1, name2, entity_type))]
    pub async fn are_names_equivalent(
        &self,
        name1: &str,
        name2: &str,
        entity_type: &str,
    ) -> Result<bool, sqlx::Error> {
        // If the names are identical, they're equivalent
        if name1 == name2 {
            return Ok(true);
        }

        // First, check the cache (to be implemented)

        // Check if there's a negative match
        let is_negative = self.is_negative_match(name1, name2, entity_type).await?;
        if is_negative {
            return Ok(false);
        }

        // Check if both have a canonical form and if they're the same
        let canonical1 = self.get_canonical_name(name1, entity_type).await?;
        let canonical2 = self.get_canonical_name(name2, entity_type).await?;

        if let (Some(c1), Some(c2)) = (&canonical1, &canonical2) {
            if c1 == c2 {
                // Update the cache (to be implemented)
                return Ok(true);
            }
        }

        // Check direct alias relationship
        let is_alias = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) 
            FROM entity_aliases
            WHERE 
                ((normalized_canonical = ? AND normalized_alias = ?) OR
                 (normalized_canonical = ? AND normalized_alias = ?))
                AND entity_type = ?
                AND status = 'APPROVED'
            "#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name2)
        .bind(name1)
        .bind(entity_type)
        .fetch_one(&self.pool)
        .await?;

        // Update the cache (to be implemented)
        Ok(is_alias > 0)
    }

    /// Get the canonical name for an entity alias
    #[instrument(target = "db", level = "info", skip(self, name, entity_type))]
    pub async fn get_canonical_name(
        &self,
        name: &str,
        entity_type: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar::<_, String>(
            r#"
            SELECT normalized_canonical
            FROM entity_aliases
            WHERE normalized_alias = ? AND entity_type = ? AND status = 'APPROVED'
            LIMIT 1
            "#,
        )
        .bind(name)
        .bind(entity_type)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    /// Add a negative match to prevent two entities from being considered aliases
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id1, name1, name2, entity_type)
    )]
    pub async fn add_negative_match(
        &self,
        entity_id1: i64,
        name1: &str,
        name2: &str,
        entity_type: crate::entity::types::EntityType,
        rejected_by: &str,
    ) -> Result<i64, sqlx::Error> {
        let normalizer = crate::entity::normalizer::EntityNormalizer::new();
        let normalized_name1 = normalizer.normalize(name1, entity_type);
        let normalized_name2 = normalizer.normalize(name2, entity_type);

        // Determine entity_id2 if available
        let entity_id2 = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT id FROM entities
            WHERE normalized_name = ? AND type = ?
            "#,
        )
        .bind(&normalized_name2)
        .bind(entity_type.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let rejected_at = chrono::Utc::now().to_rfc3339();

        // Insert the negative match
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO entity_negative_matches 
            (entity_id1, entity_id2, normalized_name1, normalized_name2, 
             entity_type, rejected_by, rejected_at, persistence_level)
            VALUES (?, ?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(normalized_name1, normalized_name2, entity_type) DO UPDATE SET
                entity_id1 = COALESCE(excluded.entity_id1, entity_negative_matches.entity_id1),
                entity_id2 = COALESCE(excluded.entity_id2, entity_negative_matches.entity_id2),
                rejected_by = excluded.rejected_by,
                rejected_at = excluded.rejected_at,
                persistence_level = entity_negative_matches.persistence_level + 1
            RETURNING id
            "#,
        )
        .bind(entity_id1)
        .bind(entity_id2)
        .bind(&normalized_name1)
        .bind(&normalized_name2)
        .bind(entity_type.to_string())
        .bind(rejected_by)
        .bind(rejected_at)
        .fetch_one(&self.pool)
        .await?;

        info!(target: TARGET_DB,
            "Added/updated negative match: {} ≠ {} ({}) by {}",
            name1, name2, entity_type, rejected_by
        );

        // Also delete any existing alias suggestions between these entities
        sqlx::query(
            r#"
            DELETE FROM entity_aliases
            WHERE 
                (normalized_canonical = ? AND normalized_alias = ?)
                OR (normalized_canonical = ? AND normalized_alias = ?)
                AND entity_type = ?
            "#,
        )
        .bind(&normalized_name1)
        .bind(&normalized_name2)
        .bind(&normalized_name2)
        .bind(&normalized_name1)
        .bind(entity_type.to_string())
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Check if two names are explicitly marked as not being aliases
    #[instrument(target = "db", level = "info", skip(self, name1, name2, entity_type))]
    pub async fn is_negative_match(
        &self,
        name1: &str,
        name2: &str,
        entity_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) 
            FROM entity_negative_matches
            WHERE 
                ((normalized_name1 = ? AND normalized_name2 = ?)
                 OR (normalized_name1 = ? AND normalized_name2 = ?))
                AND entity_type = ?
            "#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name2)
        .bind(name1)
        .bind(entity_type)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Update the statistics for a specific pattern type
    async fn increment_pattern_stat(
        &self,
        pattern_id: &str,
        approved: bool,
    ) -> Result<(), sqlx::Error> {
        let pattern_type = if pattern_id.starts_with("PATTERN_") {
            "REGEX"
        } else if pattern_id.starts_with("LLM_") {
            "LLM"
        } else {
            "OTHER"
        };

        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO alias_pattern_stats 
            (pattern_id, pattern_type, total_suggestions, approved_count, rejected_count, last_used_at)
            VALUES (?, ?, 1, ?, ?, ?)
            ON CONFLICT(pattern_id) DO UPDATE SET
                total_suggestions = total_suggestions + 1,
                approved_count = approved_count + ?,
                rejected_count = rejected_count + ?,
                last_used_at = excluded.last_used_at
            "#,
        )
        .bind(pattern_id)
        .bind(pattern_type)
        .bind(if approved { 1 } else { 0 })
        .bind(if !approved { 1 } else { 0 })
        .bind(now)
        .bind(if approved { 1 } else { 0 })
        .bind(if !approved { 1 } else { 0 })
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Migrate static aliases to the database
    pub async fn migrate_static_aliases(&self) -> Result<usize, sqlx::Error> {
        let mut count = 0;

        // Use a transaction for the migration
        let mut tx = self.pool.begin().await?;

        // Retrieve existing static aliases from the entity_aliases table
        let existing_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM entity_aliases WHERE source = 'STATIC'
            "#,
        )
        .fetch_one(&mut *tx)
        .await?;

        if existing_count > 0 {
            info!(target: TARGET_DB, "Found {} existing static aliases in database", existing_count);
            tx.rollback().await?;
            return Ok(existing_count as usize);
        }

        // Get person aliases
        count += Self::migrate_static_alias_type(
            &mut tx,
            self,
            crate::entity::types::EntityType::Person,
            "STATIC",
            "system_migration",
        )
        .await?;

        // Get organization aliases
        count += Self::migrate_static_alias_type(
            &mut tx,
            self,
            crate::entity::types::EntityType::Organization,
            "STATIC",
            "system_migration",
        )
        .await?;

        // Get product aliases
        count += Self::migrate_static_alias_type(
            &mut tx,
            self,
            crate::entity::types::EntityType::Product,
            "STATIC",
            "system_migration",
        )
        .await?;

        // Get location aliases
        count += Self::migrate_static_alias_type(
            &mut tx,
            self,
            crate::entity::types::EntityType::Location,
            "STATIC",
            "system_migration",
        )
        .await?;

        // Commit the transaction
        tx.commit().await?;

        info!(target: TARGET_DB, "Migrated {} static aliases to database", count);
        Ok(count)
    }

    /// Helper method to migrate a specific type of static aliases
    async fn migrate_static_alias_type(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        _db: &Database,
        entity_type: crate::entity::types::EntityType,
        source: &str,
        admin_id: &str,
    ) -> Result<usize, sqlx::Error> {
        let mut count = 0;

        // Get the static aliases for this type from the code
        let aliases = crate::entity::aliases::get_aliases_for_type(entity_type);

        for (alias_text, canonical_name) in aliases {
            let normalizer = crate::entity::normalizer::EntityNormalizer::new();
            let normalized_canonical = normalizer.normalize(&canonical_name, entity_type);
            let normalized_alias = normalizer.normalize(&alias_text, entity_type);

            // Skip if normalized forms are identical
            if normalized_canonical == normalized_alias {
                continue;
            }

            // Try to find an entity ID for the canonical name
            let entity_id = sqlx::query_scalar::<_, Option<i64>>(
                r#"
                SELECT id FROM entities 
                WHERE normalized_name = ? AND type = ?
                "#,
            )
            .bind(&normalized_canonical)
            .bind(entity_type.to_string())
            .fetch_optional(&mut **tx)
            .await?;

            let created_at = chrono::Utc::now().to_rfc3339();

            // Insert the alias
            let result = sqlx::query(
                r#"
                INSERT INTO entity_aliases
                (entity_id, canonical_name, alias_text, normalized_canonical, normalized_alias, 
                 entity_type, source, confidence, created_at, approved_by, approved_at, status)
                VALUES (?, ?, ?, ?, ?, ?, ?, 1.0, ?, ?, ?, 'APPROVED')
                ON CONFLICT (normalized_canonical, normalized_alias, entity_type) DO NOTHING
                "#,
            )
            .bind(entity_id)
            .bind(&canonical_name)
            .bind(&alias_text)
            .bind(&normalized_canonical)
            .bind(&normalized_alias)
            .bind(entity_type.to_string())
            .bind(source)
            .bind(&created_at)
            .bind(admin_id)
            .bind(&created_at)
            .execute(&mut **tx)
            .await?;

            count += result.rows_affected() as usize;
        }

        Ok(count)
    }

    /// Create a review batch for alias suggestions
    pub async fn create_alias_review_batch(&self, batch_size: i64) -> Result<i64, sqlx::Error> {
        let created_at = chrono::Utc::now().to_rfc3339();

        // Create the batch first
        let batch_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO alias_review_batches (created_at, status, total_count)
            VALUES (?, 'OPEN', 0)
            RETURNING id
            "#,
        )
        .bind(&created_at)
        .fetch_one(&self.pool)
        .await?;

        // Get pending aliases to review
        let alias_ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id FROM entity_aliases
            WHERE status = 'PENDING'
            ORDER BY confidence DESC
            LIMIT ?
            "#,
        )
        .bind(batch_size)
        .fetch_all(&self.pool)
        .await?;

        let total_count = alias_ids.len();

        // Add aliases to the batch
        for alias_id in alias_ids {
            sqlx::query(
                r#"
                INSERT INTO alias_review_items (batch_id, alias_id)
                VALUES (?, ?)
                "#,
            )
            .bind(batch_id)
            .bind(alias_id)
            .execute(&self.pool)
            .await?;
        }

        // Update the batch with the actual count
        sqlx::query(
            r#"
            UPDATE alias_review_batches 
            SET total_count = ?
            WHERE id = ?
            "#,
        )
        .bind(total_count as i64)
        .bind(batch_id)
        .execute(&self.pool)
        .await?;

        info!(target: TARGET_DB, "Created alias review batch #{} with {} items", batch_id, total_count);

        Ok(batch_id)
    }

    /// Get alias suggestions for a specific review batch
    pub async fn get_alias_review_batch(
        &self,
        batch_id: i64,
    ) -> Result<Vec<(i64, String, String, String, String, f64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT 
                a.id,
                a.canonical_name,
                a.alias_text,
                a.entity_type,
                a.source,
                a.confidence
            FROM alias_review_items ri
            JOIN entity_aliases a ON ri.alias_id = a.id
            WHERE ri.batch_id = ? AND ri.decision IS NULL
            ORDER BY a.confidence DESC
            "#,
        )
        .bind(batch_id)
        .fetch_all(&self.pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("canonical_name"),
                    row.get("alias_text"),
                    row.get("entity_type"),
                    row.get("source"),
                    row.get::<f64, _>("confidence"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Approve an alias suggestion
    pub async fn approve_alias_suggestion(
        &self,
        alias_id: i64,
        admin_id: &str,
    ) -> Result<(), sqlx::Error> {
        let approved_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE entity_aliases
            SET status = 'APPROVED', approved_by = ?, approved_at = ?
            WHERE id = ?
            "#,
        )
        .bind(admin_id)
        .bind(&approved_at)
        .bind(alias_id)
        .execute(&self.pool)
        .await?;

        info!(target: TARGET_DB, "Approved alias suggestion #{} by {}", alias_id, admin_id);

        Ok(())
    }

    /// Reject an alias suggestion
    pub async fn reject_alias_suggestion(
        &self,
        alias_id: i64,
        admin_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // Get the alias details before updating
        let row = sqlx::query(
            r#"
            SELECT canonical_name, alias_text, entity_type, entity_id
            FROM entity_aliases
            WHERE id = ?
            "#,
        )
        .bind(alias_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let canonical_name: String = row.get("canonical_name");
            let alias_text: String = row.get("alias_text");
            let entity_type: String = row.get("entity_type");
            let entity_id: Option<i64> = row.get("entity_id");

            // Update the alias status
            sqlx::query(
                r#"
                UPDATE entity_aliases
                SET status = 'REJECTED'
                WHERE id = ?
                "#,
            )
            .bind(alias_id)
            .execute(&self.pool)
            .await?;

            // Optionally add to negative matches if reason is "different entity"
            if reason == Some("different entity") && entity_id.is_some() {
                let entity_type_enum = crate::entity::types::EntityType::from_str(&entity_type)
                    .map_err(|e| {
                        sqlx::Error::Protocol(format!("Invalid entity type: {}", e).into())
                    })?;

                self.add_negative_match(
                    entity_id.unwrap(),
                    &canonical_name,
                    &alias_text,
                    entity_type_enum,
                    admin_id,
                )
                .await?;
            }

            info!(target: TARGET_DB, "Rejected alias suggestion #{} by {}: {} ↔ {} ({})", 
                 alias_id, admin_id, canonical_name, alias_text, entity_type);
        }

        Ok(())
    }

    /// Get statistics about the alias system
    pub async fn get_alias_system_stats(&self) -> Result<serde_json::Value, sqlx::Error> {
        // Get overall counts
        let total_approved: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'APPROVED'"#)
                .fetch_one(&self.pool)
                .await?;

        let total_rejected: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'REJECTED'"#)
                .fetch_one(&self.pool)
                .await?;

        let total_pending: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'PENDING'"#)
                .fetch_one(&self.pool)
                .await?;

        let negative_matches: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_negative_matches"#)
                .fetch_one(&self.pool)
                .await?;

        // Get counts by source
        let source_rows = sqlx::query(
            r#"
            SELECT source, COUNT(*) as count
            FROM entity_aliases
            GROUP BY source
            ORDER BY count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let by_source: serde_json::Map<String, serde_json::Value> = source_rows
            .into_iter()
            .map(|row| {
                let source: String = row.get("source");
                let count: i64 = row.get("count");
                (source, serde_json::Value::from(count))
            })
            .collect();

        // Get pattern statistics
        let pattern_rows = sqlx::query(
            r#"
            SELECT 
                pattern_id,
                pattern_type,
                total_suggestions,
                approved_count,
                rejected_count,
                enabled
            FROM alias_pattern_stats
            ORDER BY total_suggestions DESC
            LIMIT 20
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let pattern_stats: Vec<serde_json::Value> = pattern_rows
            .into_iter()
            .map(|row| {
                let mut pattern_data = serde_json::Map::new();
                pattern_data.insert(
                    "pattern_id".to_string(),
                    serde_json::Value::from(row.get::<String, _>("pattern_id")),
                );
                pattern_data.insert(
                    "pattern_type".to_string(),
                    serde_json::Value::from(row.get::<String, _>("pattern_type")),
                );
                pattern_data.insert(
                    "total".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("total_suggestions")),
                );
                pattern_data.insert(
                    "approved".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("approved_count")),
                );
                pattern_data.insert(
                    "rejected".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("rejected_count")),
                );
                pattern_data.insert(
                    "enabled".to_string(),
                    serde_json::Value::from(row.get::<bool, _>("enabled")),
                );

                serde_json::Value::Object(pattern_data)
            })
            .collect();

        // Get top rejected pairs
        let rejected_rows = sqlx::query(
            r#"
            SELECT 
                normalized_name1,
                normalized_name2,
                entity_type,
                COUNT(*) as rejection_count
            FROM entity_negative_matches
            GROUP BY normalized_name1, normalized_name2, entity_type
            ORDER BY rejection_count DESC
            LIMIT 10
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let top_rejected_pairs: Vec<serde_json::Value> = rejected_rows
            .into_iter()
            .map(|row| {
                let mut pair_data = serde_json::Map::new();
                pair_data.insert(
                    "name1".to_string(),
                    serde_json::Value::from(row.get::<String, _>("normalized_name1")),
                );
                pair_data.insert(
                    "name2".to_string(),
                    serde_json::Value::from(row.get::<String, _>("normalized_name2")),
                );
                pair_data.insert(
                    "entity_type".to_string(),
                    serde_json::Value::from(row.get::<String, _>("entity_type")),
                );
                pair_data.insert(
                    "rejection_count".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("rejection_count")),
                );

                serde_json::Value::Object(pair_data)
            })
            .collect();

        // Build the final JSON response
        let mut stats = serde_json::Map::new();
        stats.insert(
            "total_approved".to_string(),
            serde_json::Value::from(total_approved),
        );
        stats.insert(
            "total_rejected".to_string(),
            serde_json::Value::from(total_rejected),
        );
        stats.insert(
            "total_pending".to_string(),
            serde_json::Value::from(total_pending),
        );
        stats.insert(
            "negative_matches".to_string(),
            serde_json::Value::from(negative_matches),
        );
        stats.insert(
            "by_source".to_string(),
            serde_json::Value::Object(by_source),
        );
        stats.insert(
            "pattern_stats".to_string(),
            serde_json::Value::Array(pattern_stats),
        );
        stats.insert(
            "top_rejected_pairs".to_string(),
            serde_json::Value::Array(top_rejected_pairs),
        );

        Ok(serde_json::Value::Object(stats))
    }
}
