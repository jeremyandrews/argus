use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
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
            "#,
        )
        .execute(&mut conn)
        .await?;
        info!(target: TARGET_DB, "Articles table ensured to exist");

        Ok(Database { pool })
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
}
