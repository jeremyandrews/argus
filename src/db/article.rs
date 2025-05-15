use crate::db::Row;
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, instrument};
use url::Url;
use urlnorm::UrlNormalizer;

use super::core::{Database, DbLockErrorExt};
use crate::TARGET_DB;

impl Database {
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
            ON CONFLICT(normalized_url) DO UPDATE SET
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
        .fetch_one(self.pool())
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
        .execute(self.pool())
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
            .fetch_optional(self.pool())
            .await?;

        let seen = row.is_some();
        debug!(target: TARGET_DB, "Article seen status for {}: {}", normalized_url, seen);
        Ok(seen)
    }

    // Check if the hash of the text has already been seen, to filter out articles that
    // have multiple URLs for the same identical text.
    pub async fn has_hash(&self, hash: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 FROM articles WHERE hash = ?1")
            .bind(hash)
            .fetch_optional(self.pool())
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
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some())
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
        .fetch_optional(self.pool())
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
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            let pub_date: Option<String> = row.get("pub_date");
            let event_date: Option<String> = row.get("event_date");

            Ok((pub_date, event_date))
        } else {
            Ok((None, None))
        }
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
        .fetch_all(self.pool())
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
        let rows = query_builder.fetch_all(self.pool()).await?;
        let unseen_articles: Vec<String> = rows.into_iter().map(|row| row.get("r2_url")).collect();

        info!("Fetched unseen articles: {:?}", unseen_articles);
        if unseen_articles.is_empty() {
            info!("No unseen articles found for the given list of seen articles and device_id.");
        }

        Ok(unseen_articles)
    }
}
