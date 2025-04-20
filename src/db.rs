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
use tracing::{debug, error, info, instrument, warn};
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
    ) -> Result<Vec<(i64, Option<String>, Option<String>, Option<i64>, i64, i64)>, sqlx::Error>
    {
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

        // Convert rows to tuples
        let results = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get("quality_score"),
                    row.get::<i64, _>("primary_count"),
                    row.get::<i64, _>("total_count"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Get articles that share significant entities with the given entity IDs
    /// and are newer than the given date threshold
    pub async fn get_articles_by_entities_with_date(
        &self,
        entity_ids: &[i64],
        limit: u64,
        date_threshold: &str,
    ) -> Result<Vec<(i64, Option<String>, Option<String>, Option<i64>, i64, i64)>, sqlx::Error>
    {
        // Log the search criteria
        info!(target: TARGET_DB, "Looking for articles with entities: {:?}, date threshold: {}, limit: {}", 
              entity_ids, date_threshold, limit);

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

        // Query for articles that share entities, prioritizing those with PRIMARY importance
        // and filtering by date threshold
        let query = r#"
            SELECT a.id, a.pub_date as published_date, a.category, a.quality_score,
                   COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                   COUNT(ae.entity_id) as total_count
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
            AND a.pub_date > ?
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#;

        info!(target: TARGET_DB, "Entity search query: {}", query);

        // Log some sample pub_dates from the database
        let sample_dates: Vec<String> = match sqlx::query_scalar(
            "SELECT pub_date FROM articles WHERE pub_date IS NOT NULL LIMIT 5",
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(dates) => dates,
            Err(e) => {
                warn!(target: TARGET_DB, "Failed to fetch sample article dates: {}", e);
                vec![]
            }
        };
        info!(target: TARGET_DB, "Sample article dates for comparison: {:?}", sample_dates);

        let rows = sqlx::query(query)
            .bind(&entity_ids_json)
            .bind(date_threshold)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

        info!(target: TARGET_DB, "Entity search returned {} results for entities: {:?} (with date filter > {})", 
            rows.len(), entity_ids, date_threshold);

        // If we got no results with date filter but had matches without it, log this critical info
        if rows.is_empty() && total_matching > 0 {
            error!(target: TARGET_DB,
                "CRITICAL: Date filter is eliminating all potential matches! Found {} matching articles but 0 after date filter", 
                total_matching);

            // Let's log a few of the matching articles that were filtered out
            let sample_query = r#"
                SELECT a.id, a.pub_date, a.category,
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
                        let category: Option<String> = row.get("category");
                        let primary_count: i64 = row.get("primary_count");
                        let total_count: i64 = row.get("total_count");

                        info!(target: TARGET_DB,
                            "Example match filtered by date: article_id={}, pub_date={}, category={}, primary_count={}, total_count={}",
                            id, pub_date.unwrap_or_default(), category.unwrap_or_default(), primary_count, total_count);
                    }
                }
                Err(e) => {
                    error!(target: TARGET_DB, "Failed to get sample filtered matches: {}", e);
                }
            }
        }

        // Convert rows to tuples
        let results = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get("quality_score"),
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
}
