use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::{debug, info};

use crate::db::Database;
use crate::TARGET_DB;

/// Represents a timeout event from production LLM usage
#[derive(Debug, Clone)]
pub struct TimeoutEvent {
    pub endpoint_url: String,
    pub model_name: String,
    pub worker_id: String,
    pub worker_type: String,
    pub timeout_type: String,
    pub occurred_at: DateTime<Utc>,
}

/// Represents an alert state for an endpoint
#[derive(Debug, Clone)]
pub struct EndpointAlert {
    pub id: Option<i64>,
    pub endpoint_url: String,
    pub model_name: String,
    pub alert_type: String,
    pub first_occurrence: DateTime<Utc>,
    pub last_occurrence: DateTime<Utc>,
    pub last_alert_sent: Option<DateTime<Utc>>,
    pub occurrence_count: i32,
    pub consecutive_failures: i32,
    pub is_resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Convert DateTime<Utc> to SQLite timestamp string
fn datetime_to_string(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Convert SQLite timestamp string to DateTime<Utc>
fn string_to_datetime(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f")
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

impl Database {
    /// Record a timeout event from production LLM usage
    pub async fn record_timeout_event(&self, event: &TimeoutEvent) -> Result<(), sqlx::Error> {
        debug!(
            target: TARGET_DB,
            "Recording timeout event for {}/{} from worker {} ({})",
            event.endpoint_url, event.model_name, event.worker_id, event.worker_type
        );

        let occurred_at_str = datetime_to_string(event.occurred_at);

        sqlx::query(
            r#"
            INSERT INTO endpoint_timeout_events 
            (endpoint_url, model_name, worker_id, worker_type, timeout_type, occurred_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&event.endpoint_url)
        .bind(&event.model_name)
        .bind(&event.worker_id)
        .bind(&event.worker_type)
        .bind(&event.timeout_type)
        .bind(&occurred_at_str)
        .execute(self.pool())
        .await?;

        info!(
            target: TARGET_DB,
            "Recorded timeout event for endpoint {}/{}",
            event.endpoint_url, event.model_name
        );

        Ok(())
    }

    /// Get or create an alert record for an endpoint
    pub async fn get_or_create_alert(
        &self,
        endpoint_url: &str,
        model_name: &str,
        alert_type: &str,
    ) -> Result<EndpointAlert, sqlx::Error> {
        // First try to get existing alert
        if let Some(alert) = self
            .get_active_alert(endpoint_url, model_name, alert_type)
            .await?
        {
            return Ok(alert);
        }

        // Create new alert
        let now = Utc::now();
        let now_str = datetime_to_string(now);

        let id = sqlx::query(
            r#"
            INSERT INTO endpoint_alerts 
            (endpoint_url, model_name, alert_type, first_occurrence, last_occurrence, 
             occurrence_count, consecutive_failures, is_resolved)
            VALUES (?1, ?2, ?3, ?4, ?5, 1, 1, 0)
            "#,
        )
        .bind(endpoint_url)
        .bind(model_name)
        .bind(alert_type)
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?
        .last_insert_rowid();

        info!(
            target: TARGET_DB,
            "Created new alert for endpoint {}/{} (id: {})",
            endpoint_url, model_name, id
        );

        Ok(EndpointAlert {
            id: Some(id),
            endpoint_url: endpoint_url.to_string(),
            model_name: model_name.to_string(),
            alert_type: alert_type.to_string(),
            first_occurrence: now,
            last_occurrence: now,
            last_alert_sent: None,
            occurrence_count: 1,
            consecutive_failures: 1,
            is_resolved: false,
            resolved_at: None,
        })
    }

    /// Get active (unresolved) alert for an endpoint
    pub async fn get_active_alert(
        &self,
        endpoint_url: &str,
        model_name: &str,
        alert_type: &str,
    ) -> Result<Option<EndpointAlert>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, endpoint_url, model_name, alert_type, first_occurrence, last_occurrence,
                   last_alert_sent, occurrence_count, consecutive_failures, is_resolved, resolved_at
            FROM endpoint_alerts 
            WHERE endpoint_url = ?1 AND model_name = ?2 AND alert_type = ?3 AND is_resolved = 0
            ORDER BY last_occurrence DESC
            LIMIT 1
            "#,
        )
        .bind(endpoint_url)
        .bind(model_name)
        .bind(alert_type)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            let first_occurrence_str: String = row.get("first_occurrence");
            let last_occurrence_str: String = row.get("last_occurrence");
            let last_alert_sent_str: Option<String> = row.get("last_alert_sent");
            let resolved_at_str: Option<String> = row.get("resolved_at");

            Ok(Some(EndpointAlert {
                id: Some(row.get("id")),
                endpoint_url: row.get("endpoint_url"),
                model_name: row.get("model_name"),
                alert_type: row.get("alert_type"),
                first_occurrence: string_to_datetime(&first_occurrence_str)
                    .unwrap_or_else(Utc::now),
                last_occurrence: string_to_datetime(&last_occurrence_str).unwrap_or_else(Utc::now),
                last_alert_sent: last_alert_sent_str.and_then(|s| string_to_datetime(&s)),
                occurrence_count: row.get("occurrence_count"),
                consecutive_failures: row.get("consecutive_failures"),
                is_resolved: row.get("is_resolved"),
                resolved_at: resolved_at_str.and_then(|s| string_to_datetime(&s)),
            }))
        } else {
            Ok(None)
        }
    }

    /// Update an existing alert with new failure information
    pub async fn update_alert_failure(
        &self,
        alert_id: i64,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let occurred_at_str = datetime_to_string(occurred_at);

        sqlx::query(
            r#"
            UPDATE endpoint_alerts 
            SET last_occurrence = ?1, 
                occurrence_count = occurrence_count + 1,
                consecutive_failures = consecutive_failures + 1
            WHERE id = ?2
            "#,
        )
        .bind(&occurred_at_str)
        .bind(alert_id)
        .execute(self.pool())
        .await?;

        debug!(
            target: TARGET_DB,
            "Updated alert {} with new failure at {}",
            alert_id, occurred_at
        );

        Ok(())
    }

    /// Mark that an alert notification was sent
    pub async fn mark_alert_sent(
        &self,
        alert_id: i64,
        sent_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let sent_at_str = datetime_to_string(sent_at);

        sqlx::query(
            r#"
            UPDATE endpoint_alerts 
            SET last_alert_sent = ?1
            WHERE id = ?2
            "#,
        )
        .bind(&sent_at_str)
        .bind(alert_id)
        .execute(self.pool())
        .await?;

        info!(
            target: TARGET_DB,
            "Marked alert {} as sent at {}",
            alert_id, sent_at
        );

        Ok(())
    }

    /// Resolve an alert when the endpoint recovers
    pub async fn resolve_alert(
        &self,
        endpoint_url: &str,
        model_name: &str,
        alert_type: &str,
    ) -> Result<Option<EndpointAlert>, sqlx::Error> {
        // Get the active alert first
        if let Some(mut alert) = self
            .get_active_alert(endpoint_url, model_name, alert_type)
            .await?
        {
            let resolved_at = Utc::now();
            let resolved_at_str = datetime_to_string(resolved_at);

            sqlx::query(
                r#"
                UPDATE endpoint_alerts 
                SET is_resolved = 1, resolved_at = ?1
                WHERE id = ?2
                "#,
            )
            .bind(&resolved_at_str)
            .bind(alert.id.unwrap())
            .execute(self.pool())
            .await?;

            alert.is_resolved = true;
            alert.resolved_at = Some(resolved_at);

            info!(
                target: TARGET_DB,
                "Resolved alert for endpoint {}/{} after {} minutes",
                endpoint_url, model_name,
                (resolved_at - alert.first_occurrence).num_minutes()
            );

            Ok(Some(alert))
        } else {
            Ok(None)
        }
    }

    /// Get timeout statistics for an endpoint over a time period
    pub async fn get_timeout_stats(
        &self,
        endpoint_url: &str,
        model_name: &str,
        since: DateTime<Utc>,
    ) -> Result<(i32, Option<DateTime<Utc>>, Option<DateTime<Utc>>), sqlx::Error> {
        let since_str = datetime_to_string(since);

        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count, MIN(occurred_at) as first_timeout, MAX(occurred_at) as last_timeout
            FROM endpoint_timeout_events 
            WHERE endpoint_url = ?1 AND model_name = ?2 AND occurred_at >= ?3
            "#,
        )
        .bind(endpoint_url)
        .bind(model_name)
        .bind(&since_str)
        .fetch_one(self.pool())
        .await?;

        let count: i32 = row.get("count");
        let first_timeout_str: Option<String> = row.get("first_timeout");
        let last_timeout_str: Option<String> = row.get("last_timeout");

        let first_timeout = first_timeout_str.and_then(|s| string_to_datetime(&s));
        let last_timeout = last_timeout_str.and_then(|s| string_to_datetime(&s));

        Ok((count, first_timeout, last_timeout))
    }

    /// Clean up old timeout events (keep only recent data)
    pub async fn cleanup_old_timeout_events(&self, days_to_keep: i32) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now() - chrono::Duration::days(days_to_keep as i64);
        let cutoff_str = datetime_to_string(cutoff);

        let result = sqlx::query(
            r#"
            DELETE FROM endpoint_timeout_events 
            WHERE occurred_at < ?1
            "#,
        )
        .bind(&cutoff_str)
        .execute(self.pool())
        .await?;

        let deleted_count = result.rows_affected();

        if deleted_count > 0 {
            info!(
                target: TARGET_DB,
                "Cleaned up {} old timeout events (older than {} days)",
                deleted_count, days_to_keep
            );
        }

        Ok(deleted_count)
    }

    /// Clean up resolved alerts older than specified days
    pub async fn cleanup_old_resolved_alerts(&self, days_to_keep: i32) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now() - chrono::Duration::days(days_to_keep as i64);
        let cutoff_str = datetime_to_string(cutoff);

        let result = sqlx::query(
            r#"
            DELETE FROM endpoint_alerts 
            WHERE is_resolved = 1 AND resolved_at < ?1
            "#,
        )
        .bind(&cutoff_str)
        .execute(self.pool())
        .await?;

        let deleted_count = result.rows_affected();

        if deleted_count > 0 {
            info!(
                target: TARGET_DB,
                "Cleaned up {} old resolved alerts (older than {} days)",
                deleted_count, days_to_keep
            );
        }

        Ok(deleted_count)
    }
}
