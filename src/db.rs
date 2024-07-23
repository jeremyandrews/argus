use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, instrument};

use crate::TARGET_DB;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    #[instrument(target = "db", level = "info")]
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        info!(target: TARGET_DB, "Creating database pool for: {}", database_url);

        // Check if the database file exists
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
                url TEXT NOT NULL UNIQUE
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_url ON rss_queue (url);
            "#,
        )
        .execute(&mut conn)
        .await?;
        info!(target: TARGET_DB, "Tables ensured to exist");

        Ok(Database { pool })
    }

    #[instrument(target = "db", level = "info", skip(self, url))]
    pub async fn add_to_queue(&self, url: &str) -> Result<(), sqlx::Error> {
        info!(target: TARGET_DB, "Adding URL to queue: {}", url);
        sqlx::query(
            r#"
            INSERT INTO rss_queue (url)
            VALUES (?1)
            ON CONFLICT(url) DO NOTHING
            "#,
        )
        .bind(url)
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
    pub async fn fetch_and_delete_url_from_queue(&self) -> Result<Option<String>, sqlx::Error> {
        info!(target: TARGET_DB, "Fetching and deleting URL from queue");

        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query("SELECT url FROM rss_queue ORDER BY RANDOM() LIMIT 1")
            .fetch_optional(&mut transaction)
            .await?;

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
