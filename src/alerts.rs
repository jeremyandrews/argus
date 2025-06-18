use chrono::{DateTime, Duration, Utc};
use std::env;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

use crate::app::util::send_admin_alert;
use crate::db::alerts::{EndpointAlert, TimeoutEvent};
use crate::db::Database;
use crate::WorkerDetail;

/// Configuration for alert thresholds and behavior
#[derive(Debug, Clone)]
pub struct AlertConfig {
    pub threshold_minutes: i64, // Minutes of failures before first alert
    pub cooldown_minutes: i64,  // Minutes between subsequent alerts
    pub admin_device_token: Option<String>, // iOS device token for admin alerts
}

impl AlertConfig {
    /// Load alert configuration from environment variables
    pub fn from_env() -> Self {
        let threshold_minutes = env::var("ALERT_THRESHOLD_MINUTES")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);

        let cooldown_minutes = env::var("ALERT_COOLDOWN_MINUTES")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60);

        let admin_device_token = env::var("ADMIN_DEVICE_TOKEN").ok();

        Self {
            threshold_minutes,
            cooldown_minutes,
            admin_device_token,
        }
    }

    /// Check if admin alerts are configured
    pub fn is_configured(&self) -> bool {
        self.admin_device_token.is_some()
    }
}

/// Alert manager for handling timeout events and notifications
pub struct AlertManager {
    config: AlertConfig,
    db: &'static Database,
}

impl AlertManager {
    /// Create a new alert manager
    pub async fn new() -> Self {
        let config = AlertConfig::from_env();
        let db = Database::instance().await;

        if config.is_configured() {
            info!(
                "Alert manager initialized with {}min threshold, {}min cooldown",
                config.threshold_minutes, config.cooldown_minutes
            );
        } else {
            warn!("Alert manager initialized but no ADMIN_DEVICE_TOKEN configured - alerts will be logged only");
        }

        Self { config, db }
    }

    /// Process a timeout event from an LLM operation
    pub async fn handle_timeout(
        &self,
        worker_detail: &WorkerDetail,
        timeout_type: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let endpoint_url = &worker_detail.connection_info;
        let model_name = &worker_detail.model;
        let occurred_at = Utc::now();

        // Record the timeout event
        let timeout_event = TimeoutEvent {
            endpoint_url: endpoint_url.clone(),
            model_name: model_name.clone(),
            worker_id: worker_detail.id.to_string(),
            worker_type: worker_detail.name.clone(),
            timeout_type: timeout_type.to_string(),
            occurred_at,
        };

        self.db.record_timeout_event(&timeout_event).await?;

        // Get or create alert for this endpoint
        let mut alert = self
            .db
            .get_or_create_alert(endpoint_url, model_name, "timeout_threshold")
            .await?;

        // If this is an existing alert, update it
        if alert.occurrence_count > 1 {
            self.db
                .update_alert_failure(alert.id.unwrap(), occurred_at)
                .await?;
            // Refresh alert data
            alert = self
                .db
                .get_active_alert(endpoint_url, model_name, "timeout_threshold")
                .await?
                .unwrap();
        }

        // Check if we should send an alert
        self.check_and_send_alert(&alert).await?;

        Ok(())
    }

    /// Process a successful LLM operation (potential resolution)
    pub async fn handle_success(
        &self,
        worker_detail: &WorkerDetail,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let endpoint_url = &worker_detail.connection_info;
        let model_name = &worker_detail.model;

        // Check if there's an active alert to resolve
        if let Some(resolved_alert) = self
            .db
            .resolve_alert(endpoint_url, model_name, "timeout_threshold")
            .await?
        {
            self.send_resolution_alert(&resolved_alert).await?;
        }

        Ok(())
    }

    /// Check if an alert should be sent based on thresholds and cooldowns
    async fn check_and_send_alert(
        &self,
        alert: &EndpointAlert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = Utc::now();
        let time_since_first = now - alert.first_occurrence;
        let threshold_duration = Duration::minutes(self.config.threshold_minutes);

        // Check if enough time has passed to warrant an alert
        if time_since_first < threshold_duration {
            debug!(
                "Not enough time passed for alert on {}/{} ({}min < {}min threshold)",
                alert.endpoint_url,
                alert.model_name,
                time_since_first.num_minutes(),
                self.config.threshold_minutes
            );
            return Ok(());
        }

        // Check cooldown period
        if let Some(last_sent) = alert.last_alert_sent {
            let time_since_last_alert = now - last_sent;
            let cooldown_duration = Duration::minutes(self.config.cooldown_minutes);

            if time_since_last_alert < cooldown_duration {
                debug!(
                    "Alert cooldown active for {}/{} ({}min < {}min cooldown)",
                    alert.endpoint_url,
                    alert.model_name,
                    time_since_last_alert.num_minutes(),
                    self.config.cooldown_minutes
                );
                return Ok(());
            }
        }

        // Send the alert
        self.send_timeout_alert(alert).await?;

        // Mark alert as sent
        self.db.mark_alert_sent(alert.id.unwrap(), now).await?;

        Ok(())
    }

    /// Send a timeout alert notification
    async fn send_timeout_alert(
        &self,
        alert: &EndpointAlert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let duration_minutes = (Utc::now() - alert.first_occurrence).num_minutes();

        let title = "Argus: LLM Endpoint Issue";
        let body = format!(
            "{} on {} failing for {}min ({} timeouts)",
            alert.model_name, alert.endpoint_url, duration_minutes, alert.occurrence_count
        );

        info!("Sending timeout alert: {}", body);

        // Try to send iOS notification if configured
        if let Some(device_token) = &self.config.admin_device_token {
            match send_admin_alert(device_token, title, &body, Some(alert)).await {
                Some(_) => {
                    info!(
                        "Successfully sent iOS timeout alert for {}/{}",
                        alert.endpoint_url, alert.model_name
                    );
                }
                None => {
                    warn!(
                        "Failed to send iOS timeout alert for {}/{}",
                        alert.endpoint_url, alert.model_name
                    );
                }
            }
        } else {
            warn!(
                "Alert generated but no admin device token configured: {}",
                body
            );
        }

        Ok(())
    }

    /// Send a resolution alert notification
    async fn send_resolution_alert(
        &self,
        alert: &EndpointAlert,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(resolved_at) = alert.resolved_at {
            let downtime_minutes = (resolved_at - alert.first_occurrence).num_minutes();

            let title = "Argus: Endpoint Recovered";
            let body = format!(
                "{} on {} is back online (was down {}min)",
                alert.model_name, alert.endpoint_url, downtime_minutes
            );

            info!("Sending resolution alert: {}", body);

            // Try to send iOS notification if configured
            if let Some(device_token) = &self.config.admin_device_token {
                match send_admin_alert(device_token, title, &body, Some(alert)).await {
                    Some(_) => {
                        info!(
                            "Successfully sent iOS resolution alert for {}/{}",
                            alert.endpoint_url, alert.model_name
                        );
                    }
                    None => {
                        warn!(
                            "Failed to send iOS resolution alert for {}/{}",
                            alert.endpoint_url, alert.model_name
                        );
                    }
                }
            } else {
                info!(
                    "Endpoint recovered but no admin device token configured: {}",
                    body
                );
            }
        }

        Ok(())
    }

    /// Get timeout statistics for reporting
    pub async fn get_endpoint_stats(
        &self,
        endpoint_url: &str,
        model_name: &str,
        hours: i64,
    ) -> Result<(i32, Option<DateTime<Utc>>, Option<DateTime<Utc>>), sqlx::Error> {
        let since = Utc::now() - Duration::hours(hours);
        self.db
            .get_timeout_stats(endpoint_url, model_name, since)
            .await
    }

    /// Clean up old data
    pub async fn cleanup_old_data(&self) -> Result<(), sqlx::Error> {
        let timeout_events_deleted = self.db.cleanup_old_timeout_events(7).await?;
        let alerts_deleted = self.db.cleanup_old_resolved_alerts(30).await?;

        if timeout_events_deleted > 0 || alerts_deleted > 0 {
            info!(
                "Alert cleanup completed: {} timeout events, {} resolved alerts removed",
                timeout_events_deleted, alerts_deleted
            );
        }

        Ok(())
    }
}

/// Global alert manager instance using safe initialization
static ALERT_MANAGER: OnceLock<AlertManager> = OnceLock::new();

/// Initialize the global alert manager
pub async fn init_alert_manager() {
    let manager = AlertManager::new().await;
    if ALERT_MANAGER.set(manager).is_err() {
        warn!("Alert manager was already initialized");
    }
}

/// Get the global alert manager instance
pub fn get_alert_manager() -> Option<&'static AlertManager> {
    ALERT_MANAGER.get()
}

/// Handle a timeout event (convenience function)
pub async fn handle_timeout_event(
    worker_detail: &WorkerDetail,
    timeout_type: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(manager) = get_alert_manager() {
        manager.handle_timeout(worker_detail, timeout_type).await
    } else {
        warn!("Alert manager not initialized, timeout event not recorded");
        Ok(())
    }
}

/// Handle a successful LLM operation (convenience function)
pub async fn handle_success_event(
    worker_detail: &WorkerDetail,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(manager) = get_alert_manager() {
        manager.handle_success(worker_detail).await
    } else {
        // Success events are less critical to log if manager isn't initialized
        Ok(())
    }
}
