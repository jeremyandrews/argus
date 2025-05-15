use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Sqlite,
};
use std::path::Path;
use std::str::FromStr;
use tokio::sync::OnceCell;
use tokio::time::Duration;
use tracing::{info, instrument};

use crate::TARGET_DB;

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
pub trait DbLockErrorExt {
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

        // Initialize schema
        let db = Database { pool };
        db.initialize_schema().await?;

        Ok(db)
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
}
