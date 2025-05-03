use crate::db::Row;
use chrono::{DateTime, TimeZone, Utc};

use super::core::Database;
use crate::SubscriptionInfo;

impl Database {
    /// Add a new device to the `devices` table (returns the device ID's internal `id`)
    pub async fn add_device(&self, device_id: &str) -> Result<i64, sqlx::Error> {
        // Attempt to insert the device
        let result = sqlx::query(
            r#"
            INSERT INTO devices (device_id)
            VALUES (?1)
            ON CONFLICT(device_id) DO NOTHING
            RETURNING id;
            "#,
        )
        .bind(device_id)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = result {
            // If the insert was successful, return the newly inserted id
            Ok(row.get("id"))
        } else {
            // If the device already exists, retrieve its id
            let existing_id = sqlx::query_scalar(
                r#"
                SELECT id FROM devices WHERE device_id = ?1;
                "#,
            )
            .bind(device_id)
            .fetch_one(self.pool())
            .await?;
            Ok(existing_id)
        }
    }

    /// Subscribe a device to a specific topic
    pub async fn subscribe_to_topic(
        &self,
        device_id: &str,
        topic: &str,
        priority: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let device_id_internal = self.add_device(device_id).await?;
        sqlx::query(
            r#"
            INSERT INTO device_subscriptions (device_id, topic, priority)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(device_id, topic) DO UPDATE SET priority = ?3
            "#,
        )
        .bind(device_id_internal)
        .bind(topic)
        .bind(priority)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Unsubscribe a device from a specific topic
    pub async fn unsubscribe_from_topic(
        &self,
        device_id: &str,
        topic: &str,
    ) -> Result<bool, sqlx::Error> {
        let rows_affected = sqlx::query(
            r#"
            DELETE FROM device_subscriptions
            WHERE device_id = (SELECT id FROM devices WHERE device_id = ?1)
            AND topic = ?2;
            "#,
        )
        .bind(device_id)
        .bind(topic)
        .execute(self.pool())
        .await?
        .rows_affected();

        // Return true if a subscription was removed, false otherwise
        Ok(rows_affected > 0)
    }

    /// Remove a device token and its subscriptions from the database
    pub async fn remove_device_token(&self, device_token: &str) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool().begin().await?;

        // Delete subscriptions for the device
        sqlx::query(
            r#"
                DELETE FROM device_subscriptions
                WHERE device_id = (SELECT id FROM devices WHERE device_id = ?1);
                "#,
        )
        .bind(device_token)
        .execute(&mut *transaction)
        .await?;

        // Delete the device itself
        sqlx::query(
            r#"
                DELETE FROM devices
                WHERE device_id = ?1;
                "#,
        )
        .bind(device_token)
        .execute(&mut *transaction)
        .await?;

        // Commit the transaction
        transaction.commit().await?;
        Ok(())
    }

    /// Fetch all device IDs subscribed to a specific topic with high priority
    pub async fn fetch_devices_for_topic(
        &self,
        topic: &str,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT d.device_id, COALESCE(ds.priority, 'low') as priority
            FROM device_subscriptions ds
            JOIN devices d ON ds.device_id = d.id
            WHERE ds.topic = ?1 AND (ds.priority = 'high' OR ds.priority IS NULL AND 'high' = 'high');
            "#,
        )
        .bind(topic)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get("device_id"), row.get("priority")))
            .collect())
    }

    pub async fn log_ip_address(
        &self,
        device_id: &str,
        ip_address: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp(); // This gives us the Unix timestamp
        sqlx::query(
            r#"
            INSERT INTO ip_logs (device_id, ip_address, first_seen, last_seen)
            VALUES (
                (SELECT id FROM devices WHERE device_id = ?1),
                ?2,
                ?3,
                ?3
            )
            ON CONFLICT (device_id, ip_address) DO UPDATE SET
                last_seen = excluded.last_seen
            "#,
        )
        .bind(device_id)
        .bind(ip_address)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_ip_logs_for_device(
        &self,
        device_id: &str,
    ) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i64, i64)>(
            r#"
            SELECT ip_address, first_seen, last_seen
            FROM ip_logs
            JOIN devices ON ip_logs.device_id = devices.id
            WHERE devices.device_id = ?1
            ORDER BY last_seen DESC
            "#,
        )
        .bind(device_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(ip, first, last)| {
                (
                    ip,
                    match Utc.timestamp_opt(first, 0) {
                        chrono::LocalResult::Single(dt) => dt,
                        _ => Utc::now(), // Fallback to current time if timestamp is ambiguous or out of range
                    },
                    match Utc.timestamp_opt(last, 0) {
                        chrono::LocalResult::Single(dt) => dt,
                        _ => Utc::now(), // Fallback to current time if timestamp is ambiguous or out of range
                    },
                )
            })
            .collect())
    }

    pub async fn get_device_subscriptions(
        &self,
        device_id: &str,
    ) -> Result<Vec<SubscriptionInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT ds.topic, COALESCE(ds.priority, 'low') as priority
            FROM device_subscriptions ds
            JOIN devices d ON ds.device_id = d.id
            WHERE d.device_id = ?1
            ORDER BY ds.topic;
            "#,
        )
        .bind(device_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(topic, priority)| SubscriptionInfo { topic, priority })
            .collect())
    }
}
