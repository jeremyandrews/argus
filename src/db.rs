use once_cell::sync::OnceCell;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, instrument};
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

    pub fn instance() -> &'static Database {
        static INSTANCE: OnceCell<Database> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            let handle = tokio::runtime::Handle::current();
            let database_url =
                std::env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());

            // Use `spawn_blocking` to avoid blocking the async runtime
            handle.block_on(async {
                let database = Database::new(&database_url)
                    .await
                    .expect("Failed to initialize database");
                database
            })
        })
    }

    #[instrument(target = "db", level = "info", skip(self, url))]
    pub async fn add_to_queue(&self, url: &str) -> Result<(), sqlx::Error> {
        if url.trim().is_empty() {
            error!(target: TARGET_DB, "Attempted to add an empty URL to the queue");
            return Err(sqlx::Error::Protocol("Empty URL provided".into()));
        }

        if Url::parse(url).is_err() {
            error!(target: TARGET_DB, "Attempted to add an invalid URL to the queue: {}", url);
            return Err(sqlx::Error::Protocol("Invalid URL provided".into()));
        }

        let seen_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time travel")
            .as_secs()
            .to_string();

        info!(target: TARGET_DB, "Adding URL to queue: {}", url);
        sqlx::query(
            r#"
            INSERT INTO rss_queue (url, seen_at)
            VALUES (?1, ?2)
            ON CONFLICT(url) DO NOTHING
            "#,
        )
        .bind(url)
        .bind(seen_at)
        .execute(&self.pool)
        .await?;
        info!(target: TARGET_DB, "URL added to queue: {}", url);
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

        info!(target: TARGET_DB, "Adding/updating article: {}", url);
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

        info!(target: TARGET_DB, "Article added/updated: {}", url);
        Ok(())
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn has_seen(&self, url: &str) -> Result<bool, sqlx::Error> {
        info!(target: TARGET_DB, "Checking if article has been seen: {}", url);

        let row = sqlx::query("SELECT 1 FROM articles WHERE url = ?1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;

        let seen = row.is_some();
        info!(target: TARGET_DB, "Article seen status for {}: {}", url, seen);
        Ok(seen)
    }

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_url_from_queue(
        &self,
        order: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        info!(target: TARGET_DB, "Fetching and deleting URL from queue");

        let mut transaction = self.pool.begin().await?;
        let row = match order {
            "oldest" => {
                sqlx::query("SELECT url FROM rss_queue ORDER BY seen_at ASC LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
            "newest" => {
                sqlx::query("SELECT url FROM rss_queue ORDER BY seen_at DESC LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
            _ => {
                sqlx::query("SELECT url FROM rss_queue ORDER BY RANDOM() LIMIT 1")
                    .fetch_optional(&mut transaction)
                    .await?
            }
        };

        if let Some(row) = row {
            let url: String = row.get("url");
            sqlx::query("DELETE FROM rss_queue WHERE url = ?1")
                .bind(&url)
                .execute(&mut transaction)
                .await?;
            transaction.commit().await?;
            info!(target: TARGET_DB, "Fetched and deleted URL from queue: {}", url);
            Ok(Some(url))
        } else {
            info!(target: TARGET_DB, "No URL found in queue");
            transaction.rollback().await?;
            Ok(None)
        }
    }
}
