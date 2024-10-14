use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Row, Sqlite,
};
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::OnceCell;
use tokio::time::Duration;
use tracing::{debug, error, info, instrument};
use url::Url;
use urlnorm::UrlNormalizer;

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
                title_domain_hash TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_relevant_category ON articles (is_relevant, category);
            CREATE INDEX IF NOT EXISTS idx_hash ON articles (hash);
            CREATE INDEX IF NOT EXISTS idx_title_domain_hash ON articles (title_domain_hash);
        
            CREATE TABLE IF NOT EXISTS rss_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL UNIQUE,
                title TEXT,
                seen_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
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
        sqlx::query(
            r#"
            INSERT INTO articles (url, normalized_url, seen_at, is_relevant, category, analysis, tiny_summary, hash, title_domain_hash)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(url) DO UPDATE SET
                url = excluded.url,
                seen_at = excluded.seen_at,
                is_relevant = excluded.is_relevant,
                category = excluded.category,
                analysis = excluded.analysis,
                tiny_summary = excluded.tiny_summary,
                hash = excluded.hash,
                title_domain_hash = excluded.title_domain_hash
            "#,
        )
        .bind(url)
        .bind(normalized_url)
        .bind(seen_at)
        .bind(is_relevant)
        .bind(category)
        .bind(analysis)
        .bind(tiny_summary)
        .bind(hash)
        .bind(title_domain_hash)
        .execute(&self.pool)
        .await?;

        debug!(target: TARGET_DB, "Article added/updated: {}", url);
        Ok(())
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
    pub async fn fetch_and_delete_url_from_queue(
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
}
