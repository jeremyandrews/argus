use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, instrument};
use url::Url;
use urlnorm::UrlNormalizer;

use super::core::Database;
use crate::TARGET_DB;

impl Database {
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
            .fetch_optional(self.pool())
            .await?
            .is_some();

        if exists_in_articles {
            debug!(target: TARGET_DB, "URL already exists in articles: {}", normalized_url);
            return Ok(false);
        }

        // 4) Check existence in rss_queue
        let exists_in_queue = sqlx::query("SELECT 1 FROM rss_queue WHERE normalized_url = ?1")
            .bind(&normalized_url)
            .fetch_optional(self.pool())
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
        .execute(self.pool())
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
        pub_date: Option<&str>,
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
        .execute(self.pool())
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
        pub_date: Option<&str>,
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
        .execute(self.pool())
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

        let mut transaction = self.pool().begin().await?;
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

    #[instrument(target = "db", level = "info", skip(self))]
    pub async fn fetch_and_delete_url_from_rss_queue(
        &self,
        order: &str,
    ) -> Result<Option<(String, Option<String>, Option<String>)>, sqlx::Error> {
        let mut transaction = self.pool().begin().await?;
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

        let mut transaction = self.pool().begin().await?;
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
            .fetch_one(self.pool())
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
        .execute(self.pool())
        .await?
        .rows_affected();

        info!(target: TARGET_DB, "Cleaned up {} entries from the queue", affected_rows);
        Ok(affected_rows)
    }
}
