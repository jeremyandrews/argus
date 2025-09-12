use chrono::Utc;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Pool, Sqlite,
};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
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

    /// Fast statistics using persistent counters + live transient data only
    pub async fn collect_runtime_stats(&self) -> Result<String, sqlx::Error> {
        // Get persistent counters (fast)
        let total_processed = self.get_counter("articles_processed_total").await?;
        let total_relevant = self.get_counter("articles_relevant_total").await?;

        // Live counts only for transient data (fast)
        let rss_queue: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rss_queue")
            .fetch_one(self.pool())
            .await?;
        let life_safety_queue: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM life_safety_queue")
            .fetch_one(self.pool())
            .await?;
        let matched_topics_queue: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM matched_topics_queue")
                .fetch_one(self.pool())
                .await?;
        let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
            .fetch_one(self.pool())
            .await?;

        let total_irrelevant = total_processed - total_relevant;

        // Format for backwards compatibility
        Ok([
            total_irrelevant,
            total_relevant,
            rss_queue,
            life_safety_queue,
            matched_topics_queue,
            devices,
        ]
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(":"))
    }

    /// Increment a system counter
    pub async fn increment_counter(
        &self,
        counter_name: &str,
        increment: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO system_counters (counter_name, counter_value, last_updated)
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(counter_name) DO UPDATE SET 
             counter_value = counter_value + ?2, 
             last_updated = CURRENT_TIMESTAMP",
        )
        .bind(counter_name)
        .bind(increment)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Get current counter value
    pub async fn get_counter(&self, counter_name: &str) -> Result<i64, sqlx::Error> {
        let result =
            sqlx::query_scalar("SELECT counter_value FROM system_counters WHERE counter_name = ?1")
                .bind(counter_name)
                .fetch_optional(self.pool())
                .await?
                .unwrap_or(0);
        Ok(result)
    }

    /// Set a counter to a specific value (used for initialization)
    pub async fn set_counter(&self, counter_name: &str, value: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO system_counters (counter_name, counter_value, last_updated)
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(counter_name) DO UPDATE SET 
             counter_value = ?2, 
             last_updated = CURRENT_TIMESTAMP",
        )
        .bind(counter_name)
        .bind(value)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Record daily processing statistics
    pub async fn record_daily_stats(&self, date: &str) -> Result<(), sqlx::Error> {
        // This will aggregate daily metrics - implementation can be added later
        // For now, just ensure the date exists in processing_statistics
        sqlx::query("INSERT OR IGNORE INTO processing_statistics (date) VALUES (?1)")
            .bind(date)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Initialize system counters from existing database data
    pub async fn initialize_statistics_from_existing_data(&self) -> Result<(), sqlx::Error> {
        tracing::info!(target: TARGET_DB, "Initializing system counters from existing database data");

        let existing_processed = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM articles")
            .fetch_one(self.pool())
            .await?;
        let existing_relevant =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM articles WHERE is_relevant = 1")
                .fetch_one(self.pool())
                .await?;
        let existing_entities =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM article_entities")
                .fetch_one(self.pool())
                .await?;

        // Set initial counter values
        self.set_counter("articles_processed_total", existing_processed)
            .await?;
        self.set_counter("articles_relevant_total", existing_relevant)
            .await?;
        self.set_counter("entities_extracted_total", existing_entities)
            .await?;

        tracing::info!(
            target: TARGET_DB,
            "Initialized statistics: {} processed, {} relevant, {} entities",
            existing_processed, existing_relevant, existing_entities
        );

        Ok(())
    }

    /// Clean up old and processed queue entries (Daily Maintenance)
    pub async fn cleanup_queues_daily(&self) -> Result<(u64, u64, u64), sqlx::Error> {
        tracing::info!(target: TARGET_DB, "Starting daily queue cleanup");

        // RSS Queue: Remove entries > 7 days old (likely stale)
        let rss_cleaned = sqlx::query(
            "DELETE FROM rss_queue WHERE datetime(seen_at, 'unixepoch') < datetime('now', '-7 days')"
        ).execute(self.pool()).await?.rows_affected();

        // Matched Topics Queue: Remove processed entries > 24 hours old
        let matched_cleaned = sqlx::query(
            "DELETE FROM matched_topics_queue WHERE datetime(timestamp, 'unixepoch') < datetime('now', '-1 day')"
        ).execute(self.pool()).await?.rows_affected();

        // Life Safety Queue: Remove processed entries > 24 hours old
        let life_safety_cleaned = sqlx::query(
            "DELETE FROM life_safety_queue WHERE datetime(timestamp, 'unixepoch') < datetime('now', '-1 day')"
        ).execute(self.pool()).await?.rows_affected();

        // Update counter
        let total_cleaned = rss_cleaned + matched_cleaned + life_safety_cleaned;
        if total_cleaned > 0 {
            self.increment_counter("cleanup_operations_total", 1)
                .await?;
        }

        tracing::info!(
            target: TARGET_DB,
            "Daily queue cleanup completed: {} RSS, {} matched topics, {} life safety entries removed",
            rss_cleaned, matched_cleaned, life_safety_cleaned
        );

        Ok((rss_cleaned, matched_cleaned, life_safety_cleaned))
    }

    /// Clean up expired alias cache entries (Daily Maintenance)
    pub async fn cleanup_alias_cache_daily(&self) -> Result<u64, sqlx::Error> {
        let cleaned = sqlx::query(
            "DELETE FROM alias_cache_stats 
             WHERE datetime(last_accessed) < datetime('now', '-30 days')",
        )
        .execute(self.pool())
        .await?
        .rows_affected();

        if cleaned > 0 {
            tracing::info!(target: TARGET_DB, "Cleaned {} expired alias cache entries", cleaned);
        }

        Ok(cleaned)
    }

    /// Remove old articles with comprehensive safety checks (Weekly Maintenance)
    pub async fn cleanup_old_articles(
        &self,
        days_to_keep: i32,
        dry_run: bool,
    ) -> Result<u64, sqlx::Error> {
        tracing::info!(target: TARGET_DB, "Starting article cleanup (days_to_keep: {}, dry_run: {})", days_to_keep, dry_run);

        // Safety checks first - count what would be removed
        let articles_to_remove = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM articles 
             WHERE datetime(seen_at, 'unixepoch') < datetime('now', '-' || ?1 || ' days')
             AND is_relevant = 0
             AND (cluster_id IS NULL OR cluster_id NOT IN 
                  (SELECT id FROM article_clusters WHERE status = 'active'))
             AND id NOT IN (SELECT DISTINCT article_id FROM article_entities WHERE importance = 'PRIMARY')"
        )
        .bind(days_to_keep)
        .fetch_one(self.pool())
        .await?;

        if dry_run {
            tracing::info!(target: TARGET_DB, "DRY RUN: Would remove {} old articles", articles_to_remove);
            return Ok(0);
        }

        // Execute cleanup with transaction safety
        let mut tx = self.pool().begin().await?;

        let removed = sqlx::query(
            "DELETE FROM articles 
             WHERE datetime(seen_at, 'unixepoch') < datetime('now', '-' || ?1 || ' days')
             AND is_relevant = 0
             AND (cluster_id IS NULL OR cluster_id NOT IN 
                  (SELECT id FROM article_clusters WHERE status = 'active'))
             AND id NOT IN (SELECT DISTINCT article_id FROM article_entities WHERE importance = 'PRIMARY')"
        )
        .bind(days_to_keep)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        // Record cleanup statistics
        let today = Utc::now().format("%Y-%m-%d").to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO processing_statistics (date, cleanup_articles_removed) 
             VALUES (?1, ?2)
             ON CONFLICT(date) DO UPDATE SET cleanup_articles_removed = cleanup_articles_removed + ?2"
        )
        .bind(&today)
        .bind(removed as i64)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        tracing::info!(target: TARGET_DB, "Article cleanup completed: {} articles removed", removed);

        Ok(removed)
    }

    /// Clean up old entity-related data (Weekly Maintenance)
    pub async fn cleanup_entity_system_weekly(
        &self,
        dry_run: bool,
    ) -> Result<(u64, u64, u64), sqlx::Error> {
        let (negative_matches_removed, pattern_stats_disabled, review_batches_archived) = if dry_run
        {
            // Count what would be cleaned up
            let negative_matches_removed = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM entity_negative_matches 
                 WHERE persistence_level = 1 
                 AND datetime(rejected_at) < datetime('now', '-90 days')",
            )
            .fetch_one(self.pool())
            .await? as u64;

            let pattern_stats_disabled = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM alias_pattern_stats 
                 WHERE total_suggestions = 0 
                 AND datetime(last_used_at) < datetime('now', '-6 months')",
            )
            .fetch_one(self.pool())
            .await? as u64;

            let review_batches_archived = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM alias_review_batches 
                 WHERE status = 'COMPLETED' 
                 AND datetime(created_at) < datetime('now', '-60 days')",
            )
            .fetch_one(self.pool())
            .await? as u64;

            tracing::info!(
                target: TARGET_DB,
                "DRY RUN: Entity cleanup would remove {} negative matches, disable {} patterns, archive {} batches",
                negative_matches_removed, pattern_stats_disabled, review_batches_archived
            );

            (
                negative_matches_removed,
                pattern_stats_disabled,
                review_batches_archived,
            )
        } else {
            // Remove low-persistence negative matches > 90 days old
            let negative_matches_removed = sqlx::query(
                "DELETE FROM entity_negative_matches 
                 WHERE persistence_level = 1 
                 AND datetime(rejected_at) < datetime('now', '-90 days')",
            )
            .execute(self.pool())
            .await?
            .rows_affected();

            // Disable unused alias patterns > 6 months old
            let pattern_stats_disabled = sqlx::query(
                "UPDATE alias_pattern_stats 
                 SET enabled = FALSE 
                 WHERE total_suggestions = 0 
                 AND datetime(last_used_at) < datetime('now', '-6 months')",
            )
            .execute(self.pool())
            .await?
            .rows_affected();

            // Archive completed review batches > 60 days old
            let review_batches_archived = sqlx::query(
                "DELETE FROM alias_review_batches 
                 WHERE status = 'COMPLETED' 
                 AND datetime(created_at) < datetime('now', '-60 days')",
            )
            .execute(self.pool())
            .await?
            .rows_affected();

            tracing::info!(
                target: TARGET_DB,
                "Entity cleanup completed: {} negative matches removed, {} patterns disabled, {} batches archived",
                negative_matches_removed, pattern_stats_disabled, review_batches_archived
            );

            (
                negative_matches_removed,
                pattern_stats_disabled,
                review_batches_archived,
            )
        };

        Ok((
            negative_matches_removed,
            pattern_stats_disabled,
            review_batches_archived,
        ))
    }

    /// Initialize maintenance schedule with default tasks
    pub async fn initialize_maintenance_schedule(&self) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();

        // Daily tasks (24-hour intervals)
        let daily_tasks = vec![
            ("cleanup_queues_daily", 24),
            ("cleanup_alias_cache_daily", 24),
            ("record_daily_stats", 24),
        ];

        // Weekly tasks (168-hour intervals)
        let weekly_tasks = vec![
            ("cleanup_old_articles", 168),
            ("cleanup_entity_system_weekly", 168),
        ];

        for (task_name, interval_hours) in daily_tasks.into_iter().chain(weekly_tasks) {
            sqlx::query(
                "INSERT OR IGNORE INTO maintenance_schedule 
                 (task_name, last_run, next_run, interval_hours, enabled) 
                 VALUES (?1, ?2, ?3, ?4, true)",
            )
            .bind(task_name)
            .bind(now - 3600) // Start with last_run 1 hour ago to allow immediate execution
            .bind(now) // next_run = now for immediate execution on startup
            .bind(interval_hours)
            .execute(self.pool())
            .await?;
        }

        tracing::info!(target: TARGET_DB, "Maintenance schedule initialized");
        Ok(())
    }

    /// Get all due maintenance tasks
    pub async fn get_due_maintenance_tasks(&self) -> Result<Vec<(String, i32)>, sqlx::Error> {
        let now = Utc::now().timestamp();

        let tasks = sqlx::query_as::<_, (String, i32)>(
            "SELECT task_name, interval_hours FROM maintenance_schedule 
             WHERE enabled = true AND next_run <= ?1",
        )
        .bind(now)
        .fetch_all(self.pool())
        .await?;

        Ok(tasks)
    }

    /// Mark a maintenance task as completed and schedule next run
    pub async fn mark_maintenance_task_completed(
        &self,
        task_name: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();

        // Get the interval for this task
        let interval_hours: i32 = sqlx::query_scalar(
            "SELECT interval_hours FROM maintenance_schedule WHERE task_name = ?1",
        )
        .bind(task_name)
        .fetch_one(self.pool())
        .await?;

        let next_run = now + (interval_hours as i64 * 3600);

        sqlx::query(
            "UPDATE maintenance_schedule 
             SET last_run = ?1, next_run = ?2 
             WHERE task_name = ?3",
        )
        .bind(now)
        .bind(next_run)
        .bind(task_name)
        .execute(self.pool())
        .await?;

        tracing::debug!(target: TARGET_DB, "Marked task '{}' as completed, next run in {} hours", task_name, interval_hours);
        Ok(())
    }

    /// Get queue sizes for dynamic scaling decisions
    pub async fn get_queue_sizes(&self) -> Result<QueueSizes, sqlx::Error> {
        let rss_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rss_queue")
            .fetch_one(self.pool())
            .await?;

        let life_safety_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM life_safety_queue")
            .fetch_one(self.pool())
            .await?;

        let matched_topics_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM matched_topics_queue")
                .fetch_one(self.pool())
                .await?;

        Ok(QueueSizes {
            rss: rss_count as u32,
            life_safety: life_safety_count as u32,
            matched_topics: matched_topics_count as u32,
        })
    }

    /// Calculate weighted queue pressure for scaling decisions
    pub async fn get_queue_pressure(&self) -> Result<u32, sqlx::Error> {
        let sizes = self.get_queue_sizes().await?;

        // Weighted priorities: RSS=1x, Life Safety=3x, Matched Topics=2x
        let weighted_total = (sizes.rss * 1) + (sizes.life_safety * 3) + (sizes.matched_topics * 2);

        Ok(weighted_total)
    }

    /// Create a new database instance for the pool manager using proper database abstraction
    /// This maintains separation of concerns by keeping all database functionality in db/ module
    pub async fn new_for_pool_manager() -> Result<Arc<Self>, sqlx::Error> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());
        let db = Database::new(&database_path).await?;
        Ok(Arc::new(db))
    }
}

#[derive(Debug, Clone)]
pub struct QueueSizes {
    pub rss: u32,
    pub life_safety: u32,
    pub matched_topics: u32,
}
