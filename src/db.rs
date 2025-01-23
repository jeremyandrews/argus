use rand::Rng;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Row, Sqlite,
};
use std::collections::BTreeSet;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, instrument};
use url::Url;
use urlnorm::UrlNormalizer;

use crate::TARGET_DB;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
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
                is_relevant BOOLEAN NOT NULL,
                category TEXT,
                tiny_summary TEXT,
                analysis TEXT,
                hash TEXT,
                title_domain_hash TEXT,
                r2_url TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
            CREATE INDEX IF NOT EXISTS idx_hash ON articles (hash);
            CREATE INDEX IF NOT EXISTS idx_title_domain_hash ON articles (title_domain_hash);
            CREATE INDEX IF NOT EXISTS idx_r2_url ON articles (r2_url);
            CREATE INDEX IF NOT EXISTS idx_seen_at_r2_url ON articles (seen_at, r2_url);
            CREATE INDEX IF NOT EXISTS idx_feed_id ON articles (feed_id);
        
            CREATE TABLE IF NOT EXISTS rss_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL UNIQUE,
                title TEXT,
                seen_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);

            CREATE TABLE IF NOT EXISTS matched_topics_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_text TEXT NOT NULL,
                article_html TEXT NOT NULL,
                article_url TEXT NOT NULL UNIQUE,
                article_title TEXT NOT NULL,
                topic_matched TEXT NOT NULL,
                article_hash TEXT NOT NULL,
                title_domain_hash TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_matched_topics_article_url ON matched_topics_queue (article_url);

            CREATE TABLE IF NOT EXISTS life_safety_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                article_url TEXT NOT NULL UNIQUE,
                article_title TEXT NOT NULL,
                article_text TEXT NOT NULL,
                article_html TEXT NOT NULL,
                article_hash TEXT NOT NULL,
                title_domain_hash TEXT NOT NULL,
                affected_regions TEXT,
                affected_people TEXT,
                affected_places TEXT,
                non_affected_people TEXT,
                non_affected_places TEXT,
                timestamp TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_life_safety_article_url ON life_safety_queue (article_url);

            CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL UNIQUE
            );
            
            CREATE TABLE IF NOT EXISTS device_subscriptions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id INTEGER NOT NULL,
                topic TEXT NOT NULL,
                priority TEXT,
                FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
                UNIQUE(device_id, topic)
            );
            
            CREATE INDEX IF NOT EXISTS idx_topic_device_id ON device_subscriptions (topic, device_id);
            CREATE INDEX IF NOT EXISTS idx_device_id_feed_id ON subscriptions (device_id, feed_id);
            "#,
        )
        .execute(&mut conn)
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
        .execute(&mut transaction)
        .await?;

        // Delete the device itself
        sqlx::query(
            r#"
                DELETE FROM devices
                WHERE device_id = ?1;
                "#,
        )
        .bind(device_token)
        .execute(&mut transaction)
        .await?;

        // Commit the transaction
        transaction.commit().await?;
        Ok(())
    }

    /// Fetch all device IDs subscribed to a specific topic
    pub async fn fetch_devices_for_topic(
        &self,
        topic: &str,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT d.device_id, COALESCE(ds.priority, 'low') as priority
            FROM device_subscriptions ds
            JOIN devices d ON ds.device_id = d.id
            WHERE ds.topic = ?1;
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
    pub async fn add_to_queue(&self, url: &str, title: Option<&str>) -> Result<bool, sqlx::Error> {
        if url.trim().is_empty() {
            error!(target: TARGET_DB, "Attempted to add an empty URL to the queue");
            return Err(sqlx::Error::Protocol("Empty URL provided".into()));
        }

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

        // Check if the URL already exists in the articles table
        let exists_in_articles = sqlx::query("SELECT 1 FROM articles WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_articles {
            debug!(target: TARGET_DB, "URL already exists in articles: {}", normalized_url);
            return Ok(false); // Return false since the article exists
        }

        // Check if the URL already exists in the rss_queue table
        let exists_in_queue = sqlx::query("SELECT 1 FROM rss_queue WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_queue {
            debug!(target: TARGET_DB, "URL already exists in the queue: {}", &normalized_url);
            return Ok(false); // Return false since the article exists in the queue
        }

        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        debug!(target: TARGET_DB, "Adding URL to queue: {}", normalized_url);
        sqlx::query(
            r#"
        INSERT INTO rss_queue (url, normalized_url, title, seen_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(normalized_url) DO NOTHING
        "#,
        )
        .bind(url)
        .bind(&normalized_url)
        .bind(title)
        .bind(seen_at)
        .execute(&self.pool)
        .await?;
        debug!(target: TARGET_DB, "URL added to queue: {}", normalized_url);

        Ok(true) // Return true since a new article was added
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
    ) -> Result<(), sqlx::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        let result = sqlx::query(
        r#"
        INSERT INTO matched_topics_queue (article_text, article_html, article_url, article_title, article_hash, title_domain_hash, topic_matched, timestamp)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
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
        article_url: &str,
        article_title: &str,
        article_text: &str,
        article_html: &str,
        article_hash: &str,
        title_domain_hash: &str,
        affected_regions: &BTreeSet<String>,
        affected_people: &BTreeSet<String>,
        affected_places: &BTreeSet<String>,
        non_affected_people: &BTreeSet<String>,
        non_affected_places: &BTreeSet<String>,
    ) -> Result<(), sqlx::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        let result = sqlx::query(
            r#"
        INSERT INTO life_safety_queue (
            article_url, article_title, article_text, article_html, article_hash, title_domain_hash,
            affected_regions, affected_people, affected_places,
            non_affected_people, non_affected_places, timestamp
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(article_url) DO NOTHING
        "#,
        )
        .bind(article_url)
        .bind(article_title)
        .bind(article_text)
        .bind(article_html)
        .bind(article_hash)
        .bind(title_domain_hash)
        .bind(serde_json::to_string(affected_regions).expect("failed to encode affected_regions"))
        .bind(serde_json::to_string(affected_people).expect("failed to encode affected_people"))
        .bind(serde_json::to_string(affected_places).expect("failed to encode affected_places"))
        .bind(
            serde_json::to_string(non_affected_people)
                .expect("failed to encode non_affected_people"),
        )
        .bind(
            serde_json::to_string(non_affected_places)
                .expect("failed to encode non_affected_places"),
        )
        .bind(timestamp)
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
            String,           // article_url
            String,           // article_title
            String,           // article_text
            String,           // article_html
            String,           // article_hash
            String,           // title_domain_hash
            BTreeSet<String>, // affected_regions
            BTreeSet<String>, // affected_people
            BTreeSet<String>, // affected_places
            BTreeSet<String>, // non_affected_people
            BTreeSet<String>, // non_affected_places
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
            affected_regions,
            affected_people,
            affected_places,
            non_affected_people,
            non_affected_places
        FROM life_safety_queue
        ORDER BY timestamp ASC
        LIMIT 1
        "#,
        )
        .fetch_optional(&mut transaction)
        .await?;

        if let Some(row) = row {
            let id: i64 = row.get("id");
            let article_url: String = row.get("article_url");
            let article_title: String = row.get("article_title");
            let article_text: String = row.get("article_text");
            let article_html: String = row.get("article_html");
            let article_hash: String = row.get("article_hash");
            let title_domain_hash: String = row.get("title_domain_hash");

            // Deserialize JSON strings into BTreeSet<String>
            let affected_regions: BTreeSet<String> =
                serde_json::from_str(&row.get::<String, _>("affected_regions"))
                    .expect("failed to deserialize affected_regions");
            let affected_people: BTreeSet<String> =
                serde_json::from_str(&row.get::<String, _>("affected_people"))
                    .expect("failed to deserialize affected_regions");
            let affected_places: BTreeSet<String> =
                serde_json::from_str(&row.get::<String, _>("affected_places"))
                    .expect("failed to deserialize affected_regions");
            let non_affected_people: BTreeSet<String> =
                serde_json::from_str(&row.get::<String, _>("non_affected_people"))
                    .expect("failed to deserialize affected_regions");
            let non_affected_places: BTreeSet<String> =
                serde_json::from_str(&row.get::<String, _>("non_affected_places"))
                    .expect("failed to deserialize affected_regions");

            sqlx::query("DELETE FROM life_safety_queue WHERE id = ?1")
                .bind(id)
                .execute(&mut transaction)
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
                affected_regions,
                affected_people,
                affected_places,
                non_affected_people,
                non_affected_places,
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
    ) -> Result<(), sqlx::Error> {
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
            match sqlx::query(
            r#"
            INSERT INTO articles (url, normalized_url, seen_at, is_relevant, category, analysis, tiny_summary, hash, title_domain_hash, r2_url)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(url) DO UPDATE SET
                url = excluded.url,
                seen_at = excluded.seen_at,
                is_relevant = excluded.is_relevant,
                category = excluded.category,
                analysis = excluded.analysis,
                tiny_summary = excluded.tiny_summary,
                hash = excluded.hash,
                title_domain_hash = excluded.title_domain_hash,
                r2_url = excluded.r2_url
            "#,
        )
        .bind(url)
        .bind(&normalized_url)
        .bind(&seen_at)
        .bind(is_relevant)
        .bind(category)
        .bind(analysis)
        .bind(tiny_summary)
        .bind(hash)
        .bind(title_domain_hash)
        .bind(r2_url)
        .execute(&self.pool)
        .await {
            Ok(_) => {
                debug!(target: TARGET_DB, "Article added/updated: {}", url);
                return Ok(());
            }
            Err(err) => {
                if err.is_database_lock_error() {
                    info!(target: TARGET_DB, "Database is locked, waiting {}ms before retrying attempt {}/{}: {}", backoff, attempt, max_retries, url);
                    sleep(Duration::from_millis(backoff)).await;
                    backoff = backoff.saturating_mul(2); // exponential backoff
                    if attempt == max_retries {
                        // Introduce some randomness to avoid the "thundering herd problem"
                        let random_jitter = rand::thread_rng().gen_range(0..200);
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
    ) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        debug!(target: TARGET_DB, "Fetching and deleting URL from queue");

        let mut transaction = self.pool.begin().await?;
        let row = match order {
            "oldest" => {
                info!(target: TARGET_DB, "loading oldest URL");
                sqlx::query(
                    "SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY rss_queue.seen_at ASC
                    LIMIT 1",
                )
                .fetch_optional(&mut transaction)
                .await?
            }
            "newest" => {
                info!(target: TARGET_DB, "loading newest URL");
                sqlx::query(
                    "SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY rss_queue.seen_at DESC
                    LIMIT 1",
                )
                .fetch_optional(&mut transaction)
                .await?
            }
            _ => {
                info!(target: TARGET_DB, "loading random URL");
                sqlx::query(
                    "SELECT rss_queue.url, rss_queue.normalized_url, rss_queue.title
                    FROM rss_queue
                    LEFT JOIN articles ON rss_queue.normalized_url = articles.normalized_url
                    WHERE articles.normalized_url IS NULL
                    ORDER BY RANDOM()
                    LIMIT 1",
                )
                .fetch_optional(&mut transaction)
                .await?
            }
        };

        if let Some(row) = row {
            let url: String = row.get("url");
            let normalized_url: String = row.get("normalized_url");
            let title: Option<String> = row.get("title");
            sqlx::query("DELETE FROM rss_queue WHERE normalized_url = ?1")
                .bind(&normalized_url)
                .execute(&mut transaction)
                .await?;
            transaction.commit().await?;
            debug!(target: TARGET_DB, "Fetched and deleted URL from queue: {}", url);
            Ok(Some((url, title)))
        } else {
            debug!(target: TARGET_DB, "No new URLs found in queue");
            transaction.rollback().await?;
            Ok(None)
        }
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_from_matched_topics_queue(
        &self,
    ) -> Result<Option<(String, String, String, String, String, String, String)>, sqlx::Error> {
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
            topic_matched 
        FROM matched_topics_queue 
        ORDER BY timestamp ASC 
        LIMIT 1
        "#,
        )
        .fetch_optional(&mut transaction)
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

            sqlx::query("DELETE FROM matched_topics_queue WHERE id = ?1")
                .bind(id)
                .execute(&mut transaction)
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
        let query = if seen_articles.is_empty() {
            // If no seen articles, return all subscribed articles from the last day
            "SELECT DISTINCT a.r2_url
             FROM articles a
             JOIN subscriptions s ON a.feed_id = s.feed_id
             WHERE a.r2_url IS NOT NULL
             AND s.device_id = ?
             AND datetime(a.seen_at, 'unixepoch') > datetime('now', '-1 day');"
                .to_string()
        } else {
            // Generate a query with placeholders for each seen article
            let placeholders: String = seen_articles
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT DISTINCT a.r2_url
                 FROM articles a
                 JOIN subscriptions s ON a.feed_id = s.feed_id
                 WHERE a.r2_url IS NOT NULL
                 AND s.device_id = ?
                 AND a.r2_url NOT IN ({})
                 AND datetime(a.seen_at, 'unixepoch') > datetime('now', '-1 day');",
                placeholders
            )
        };

        info!("Generated SQL query: {}", query);

        let mut query_builder = sqlx::query(&query).bind(device_id);

        // Bind seen articles if there are any
        for (index, article) in seen_articles.iter().enumerate() {
            query_builder = query_builder.bind(article);
            info!("Binding article [{}]: {}", index, article);
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
}
