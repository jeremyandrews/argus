use crate::db::Database;
use crate::TARGET_DB;
use chrono::Utc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Maintenance worker that performs scheduled database cleanup tasks
pub async fn run_maintenance_worker() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(target: TARGET_DB, "Starting Maintenance Worker");

    // Initialize database connection
    let db = Database::instance().await;

    // Initialize maintenance schedule on startup
    if let Err(e) = db.initialize_maintenance_schedule().await {
        error!(target: TARGET_DB, "Failed to initialize maintenance schedule: {}", e);
        return Err(Box::new(e));
    }

    // Initialize counters from existing data if needed (one-time operation)
    if let Err(e) = db.initialize_statistics_from_existing_data().await {
        warn!(target: TARGET_DB, "Failed to initialize statistics from existing data: {}", e);
    }

    info!(target: TARGET_DB, "Maintenance Worker initialized successfully");

    // Main maintenance loop - check every 5 minutes
    let check_interval = Duration::from_secs(300); // 5 minutes
    let mut heartbeat_counter = 0;

    loop {
        heartbeat_counter += 1;

        // Log heartbeat every 6 cycles (30 minutes)
        if heartbeat_counter % 6 == 0 {
            debug!(target: TARGET_DB, "Maintenance Worker heartbeat - checking for due tasks");
        }

        match process_maintenance_tasks(&db).await {
            Ok(tasks_processed) => {
                if tasks_processed > 0 {
                    info!(target: TARGET_DB, "Processed {} maintenance tasks", tasks_processed);
                }
            }
            Err(e) => {
                error!(target: TARGET_DB, "Error processing maintenance tasks: {}", e);
            }
        }

        sleep(check_interval).await;
    }
}

/// Process all due maintenance tasks
async fn process_maintenance_tasks(
    db: &Database,
) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let due_tasks = db.get_due_maintenance_tasks().await?;
    let mut tasks_processed = 0;

    for (task_name, _interval_hours) in due_tasks {
        info!(target: TARGET_DB, "Executing maintenance task: {}", task_name);

        let result = match task_name.as_str() {
            "cleanup_queues_daily" => match db.cleanup_queues_daily().await {
                Ok((rss, matched, life_safety)) => {
                    let total = rss + matched + life_safety;
                    if total > 0 {
                        info!(target: TARGET_DB, "Queue cleanup completed: {} total entries removed", total);
                    }
                    Ok(())
                }
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            },
            "cleanup_alias_cache_daily" => match db.cleanup_alias_cache_daily().await {
                Ok(cleaned) => {
                    if cleaned > 0 {
                        info!(target: TARGET_DB, "Alias cache cleanup completed: {} entries removed", cleaned);
                    }
                    Ok(())
                }
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            },
            "record_daily_stats" => {
                let today = Utc::now().format("%Y-%m-%d").to_string();
                match db.record_daily_stats(&today).await {
                    Ok(_) => {
                        debug!(target: TARGET_DB, "Daily stats recorded for {}", today);
                        Ok(())
                    }
                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                }
            }
            "cleanup_old_articles" => {
                // Remove articles older than 90 days (configurable)
                match db.cleanup_old_articles(90, false).await {
                    Ok(removed) => {
                        if removed > 0 {
                            info!(target: TARGET_DB, "Article cleanup completed: {} articles removed", removed);
                        }
                        Ok(())
                    }
                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                }
            }
            "cleanup_entity_system_weekly" => match db.cleanup_entity_system_weekly(false).await {
                Ok((negative_matches, patterns, batches)) => {
                    let total = negative_matches + patterns + batches;
                    if total > 0 {
                        info!(target: TARGET_DB, "Entity system cleanup completed: {} items processed", total);
                    }
                    Ok(())
                }
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            },
            _ => {
                warn!(target: TARGET_DB, "Unknown maintenance task: {}", task_name);
                Ok(())
            }
        };

        match result {
            Ok(_) => {
                // Mark task as completed and schedule next run
                if let Err(e) = db.mark_maintenance_task_completed(&task_name).await {
                    error!(target: TARGET_DB, "Failed to mark task '{}' as completed: {}", task_name, e);
                } else {
                    tasks_processed += 1;
                }
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to execute maintenance task '{}': {}", task_name, e);
            }
        }
    }

    Ok(tasks_processed)
}
