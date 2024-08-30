use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;
use tracing::{debug, error, info, instrument};
use url::Url;

use crate::TARGET_DB;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
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

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite://{}", database_url))
            .await?;
        info!(target: TARGET_DB, "Database pool created");

        let mut conn = pool.acquire().await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                seen_at TEXT NOT NULL,
                is_relevant BOOLEAN NOT NULL,
                category TEXT,
                analysis TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);

            CREATE TABLE IF NOT EXISTS rss_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                seen_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_url ON rss_queue (seen_at, url);
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

    #[instrument(target = "db", level = "info", skip(self, url, title))]
    pub async fn add_to_queue(&self, url: &str, title: Option<&str>) -> Result<(), sqlx::Error> {
        if url.trim().is_empty() {
            error!(target: TARGET_DB, "Attempted to add an empty URL to the queue");
            return Err(sqlx::Error::Protocol("Empty URL provided".into()));
        }

        if Url::parse(url).is_err() {
            error!(target: TARGET_DB, "Attempted to add an invalid URL to the queue: {}", url);
            return Err(sqlx::Error::Protocol("Invalid URL provided".into()));
        }

        // Check if the URL already exists in the articles table
        let exists_in_articles = sqlx::query("SELECT 1 FROM articles WHERE url = ?1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_articles {
            debug!(target: TARGET_DB, "URL already exists in articles: {}", url);
            return Ok(());
        }

        // Check if the URL already exists in the rss_queue table
        let exists_in_queue = sqlx::query("SELECT 1 FROM rss_queue WHERE url = ?1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?
            .is_some();

        if exists_in_queue {
            debug!(target: TARGET_DB, "URL already exists in the queue: {}", url);
            return Ok(());
        }

        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        debug!(target: TARGET_DB, "Adding URL to queue: {}", url);
        sqlx::query(
            r#"
        INSERT INTO rss_queue (url, title, seen_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(url) DO NOTHING
        "#,
        )
        .bind(url)
        .bind(title)
        .bind(seen_at)
        .execute(&self.pool)
        .await?;
        debug!(target: TARGET_DB, "URL added to queue: {}", url);
        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self, url, category, analysis))]
    pub async fn add_article(
        &self,
        url: &str,
        is_relevant: bool,
        category: Option<&str>,
        analysis: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        debug!(target: TARGET_DB, "Adding/updating article: {}", url);
        sqlx::query(
            r#"
            INSERT INTO articles (url, seen_at, is_relevant, category, analysis)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(url) DO UPDATE SET
                seen_at = excluded.seen_at,
                is_relevant = excluded.is_relevant,
                category = excluded.category,
                analysis = excluded.analysis
            "#,
        )
        .bind(url)
        .bind(seen_at)
        .bind(is_relevant)
        .bind(category)
        .bind(analysis)
        .execute(&self.pool)
        .await?;

        debug!(target: TARGET_DB, "Article added/updated: {}", url);
        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn has_seen(&self, url: &str) -> Result<bool, sqlx::Error> {
        debug!(target: TARGET_DB, "Checking if article has been seen: {}", url);

        let row = sqlx::query("SELECT 1 FROM articles WHERE url = ?1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;

        let seen = row.is_some();
        debug!(target: TARGET_DB, "Article seen status for {}: {}", url, seen);
        Ok(seen)
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_url_from_queue(
        &self,
        order: &str,
    ) -> Result<Option<(String, Option<String>)>, sqlx::Error> {
        debug!(target: TARGET_DB, "Fetching and deleting URL from queue");

        let mut transaction = self.pool.begin().await?;
        let row = match order {
            "oldest" => {
                sqlx::query("SELECT url, title FROM rss_queue ORDER BY seen_at ASC LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
            "newest" => {
                sqlx::query("SELECT url, title FROM rss_queue ORDER BY seen_at DESC LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
            _ => {
                sqlx::query("SELECT url, title FROM rss_queue ORDER BY RANDOM() LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
        };

        if let Some(row) = row {
            let url: String = row.get("url");
            let title: Option<String> = row.get("title");
            sqlx::query("DELETE FROM rss_queue WHERE url = ?1")
                .bind(&url)
                .execute(&mut transaction)
                .await?;
            transaction.commit().await?;
            debug!(target: TARGET_DB, "Fetched and deleted URL from queue: {}", url);
            Ok(Some((url, title)))
        } else {
            debug!(target: TARGET_DB, "No URL found in queue");
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
}
