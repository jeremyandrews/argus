use serde_json;
use sqlx::Row;
use std::str::FromStr;
use tracing::{debug, info, instrument};

use crate::db::core::Database;
use crate::TARGET_DB;

impl Database {
    /// Add a new entity alias to the database
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id, canonical_name, alias_text, entity_type)
    )]
    pub async fn add_entity_alias(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        alias_text: &str,
        entity_type: &str,
        source: &str,
        confidence: f64,
        status: Option<&str>,
        approved_by: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        // Normalize both names for consistent matching
        let normalizer = crate::entity::normalizer::EntityNormalizer::new();
        let entity_type_enum = crate::entity::types::EntityType::from_str(entity_type)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid entity type: {}", e).into()))?;

        let normalized_canonical = normalizer.normalize(canonical_name, entity_type_enum);
        let normalized_alias = normalizer.normalize(alias_text, entity_type_enum);

        // If the normalized forms are identical, skip adding this alias
        if normalized_canonical == normalized_alias {
            debug!(target: TARGET_DB,
                "Skipping alias with identical normalized form: {} ↔ {} ({}) = {} = {}",
                canonical_name, alias_text, entity_type, normalized_canonical, normalized_alias
            );

            // Return a "dummy" ID of 0 to indicate nothing was added but operation succeeded
            return Ok(0);
        }

        // Check for negative matches
        let is_negative_match = self
            .is_negative_match(&normalized_canonical, &normalized_alias, entity_type)
            .await?;
        if is_negative_match {
            debug!(target: TARGET_DB,
                "Skipping alias due to negative match: {} ↔ {} ({})",
                canonical_name, alias_text, entity_type
            );
            return Ok(0);
        }

        let created_at = chrono::Utc::now().to_rfc3339();
        let status = status.unwrap_or("PENDING");

        // Get the approved_at timestamp if status is APPROVED and we have an approver
        let approved_at = if status == "APPROVED" && approved_by.is_some() {
            Some(created_at.clone())
        } else {
            None
        };

        let result = sqlx::query(
            r#"
            INSERT INTO entity_aliases (
                entity_id, canonical_name, alias_text, normalized_canonical, normalized_alias, 
                entity_type, source, confidence, created_at, approved_by, approved_at, status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(normalized_canonical, normalized_alias, entity_type) DO UPDATE SET
                entity_id = COALESCE(excluded.entity_id, entity_aliases.entity_id),
                source = excluded.source,
                confidence = MAX(entity_aliases.confidence, excluded.confidence),
                status = CASE 
                    WHEN excluded.status = 'APPROVED' OR entity_aliases.status = 'APPROVED' THEN 'APPROVED'
                    WHEN excluded.status = 'REJECTED' OR entity_aliases.status = 'REJECTED' THEN 'REJECTED'
                    ELSE excluded.status
                END,
                approved_by = COALESCE(excluded.approved_by, entity_aliases.approved_by),
                approved_at = COALESCE(excluded.approved_at, entity_aliases.approved_at)
            RETURNING id
            "#,
        )
        .bind(entity_id)
        .bind(canonical_name)
        .bind(alias_text)
        .bind(normalized_canonical)
        .bind(normalized_alias)
        .bind(entity_type)
        .bind(source)
        .bind(confidence)
        .bind(created_at)
        .bind(approved_by)
        .bind(approved_at)
        .bind(status)
        .fetch_one(self.pool())
        .await?;

        let id: i64 = result.get("id");
        info!(target: TARGET_DB,
            "Added/updated entity alias: {} ↔ {} ({}) [id={}]",
            canonical_name, alias_text, entity_type, id
        );

        Ok(id)
    }

    /// Add multiple aliases with the same canonical name
    #[instrument(
        target = "db",
        level = "info",
        skip(self, canonical_name, aliases, entity_type)
    )]
    pub async fn add_multiple_aliases(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        aliases: &[&str],
        entity_type: &str,
        source: &str,
        confidence: f64,
        status: Option<&str>,
        approved_by: Option<&str>,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let mut ids = Vec::new();

        for alias in aliases {
            let id = self
                .add_entity_alias(
                    entity_id,
                    canonical_name,
                    alias,
                    entity_type,
                    source,
                    confidence,
                    status,
                    approved_by,
                )
                .await?;

            if id > 0 {
                ids.push(id);
            }
        }

        Ok(ids)
    }

    /// Add a manually created alias by an admin user
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id, canonical_name, alias_text, entity_type)
    )]
    pub async fn add_admin_alias(
        &self,
        entity_id: Option<i64>,
        canonical_name: &str,
        alias_text: &str,
        entity_type: crate::entity::types::EntityType,
        admin_id: &str,
        notes: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        // Look up entity ID if not provided but we have a canonical name
        let entity_id = if entity_id.is_none() {
            let normalizer = crate::entity::normalizer::EntityNormalizer::new();
            let normalized_name = normalizer.normalize(canonical_name, entity_type);

            // Try to find the entity
            let entity_row = sqlx::query(
                r#"
                SELECT id FROM entities
                WHERE normalized_name = ? AND type = ?
                "#,
            )
            .bind(&normalized_name)
            .bind(entity_type.to_string())
            .fetch_optional(self.pool())
            .await?;

            entity_row.map(|row| row.get("id"))
        } else {
            entity_id
        };

        // Add the alias with ADMIN source and APPROVED status
        let source = format!("ADMIN:{}", notes.unwrap_or("manual"));
        self.add_entity_alias(
            entity_id,
            canonical_name,
            alias_text,
            &entity_type.to_string(),
            &source,
            1.0, // Admin aliases get maximum confidence
            Some("APPROVED"),
            Some(admin_id),
        )
        .await
    }

    /// Check if two entity names are the same according to the alias system
    #[instrument(target = "db", level = "info", skip(self, name1, name2, entity_type))]
    pub async fn are_names_equivalent(
        &self,
        name1: &str,
        name2: &str,
        entity_type: &str,
    ) -> Result<bool, sqlx::Error> {
        // If the names are identical, they're equivalent
        if name1 == name2 {
            return Ok(true);
        }

        // First, check the cache (to be implemented)

        // Check if there's a negative match
        let is_negative = self.is_negative_match(name1, name2, entity_type).await?;
        if is_negative {
            return Ok(false);
        }

        // Check if both have a canonical form and if they're the same
        let canonical1 = self.get_canonical_name(name1, entity_type).await?;
        let canonical2 = self.get_canonical_name(name2, entity_type).await?;

        if let (Some(c1), Some(c2)) = (&canonical1, &canonical2) {
            if c1 == c2 {
                // Update the cache (to be implemented)
                return Ok(true);
            }
        }

        // Check direct alias relationship
        let is_alias = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) 
            FROM entity_aliases
            WHERE 
                ((normalized_canonical = ? AND normalized_alias = ?) OR
                 (normalized_canonical = ? AND normalized_alias = ?))
                AND entity_type = ?
                AND status = 'APPROVED'
            "#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name2)
        .bind(name1)
        .bind(entity_type)
        .fetch_one(self.pool())
        .await?;

        // Update the cache (to be implemented)
        Ok(is_alias > 0)
    }

    /// Get the canonical name for an entity alias
    #[instrument(target = "db", level = "info", skip(self, name, entity_type))]
    pub async fn get_canonical_name(
        &self,
        name: &str,
        entity_type: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar::<_, String>(
            r#"
            SELECT normalized_canonical
            FROM entity_aliases
            WHERE normalized_alias = ? AND entity_type = ? AND status = 'APPROVED'
            LIMIT 1
            "#,
        )
        .bind(name)
        .bind(entity_type)
        .fetch_optional(self.pool())
        .await?;

        Ok(result)
    }

    /// Add a negative match to prevent two entities from being considered aliases
    #[instrument(
        target = "db",
        level = "info",
        skip(self, entity_id1, name1, name2, entity_type)
    )]
    pub async fn add_negative_match(
        &self,
        entity_id1: i64,
        name1: &str,
        name2: &str,
        entity_type: crate::entity::types::EntityType,
        rejected_by: &str,
    ) -> Result<i64, sqlx::Error> {
        let normalizer = crate::entity::normalizer::EntityNormalizer::new();
        let normalized_name1 = normalizer.normalize(name1, entity_type);
        let normalized_name2 = normalizer.normalize(name2, entity_type);

        // Determine entity_id2 if available
        let entity_id2 = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT id FROM entities
            WHERE normalized_name = ? AND type = ?
            "#,
        )
        .bind(&normalized_name2)
        .bind(entity_type.to_string())
        .fetch_optional(self.pool())
        .await?;

        let rejected_at = chrono::Utc::now().to_rfc3339();

        // Insert the negative match
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO entity_negative_matches 
            (entity_id1, entity_id2, normalized_name1, normalized_name2, 
             entity_type, rejected_by, rejected_at, persistence_level)
            VALUES (?, ?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(normalized_name1, normalized_name2, entity_type) DO UPDATE SET
                entity_id1 = COALESCE(excluded.entity_id1, entity_negative_matches.entity_id1),
                entity_id2 = COALESCE(excluded.entity_id2, entity_negative_matches.entity_id2),
                rejected_by = excluded.rejected_by,
                rejected_at = excluded.rejected_at,
                persistence_level = entity_negative_matches.persistence_level + 1
            RETURNING id
            "#,
        )
        .bind(entity_id1)
        .bind(entity_id2)
        .bind(&normalized_name1)
        .bind(&normalized_name2)
        .bind(entity_type.to_string())
        .bind(rejected_by)
        .bind(rejected_at)
        .fetch_one(self.pool())
        .await?;

        info!(target: TARGET_DB,
            "Added/updated negative match: {} ≠ {} ({}) by {}",
            name1, name2, entity_type, rejected_by
        );

        // Also delete any existing alias suggestions between these entities
        sqlx::query(
            r#"
            DELETE FROM entity_aliases
            WHERE 
                (normalized_canonical = ? AND normalized_alias = ?)
                OR (normalized_canonical = ? AND normalized_alias = ?)
                AND entity_type = ?
            "#,
        )
        .bind(&normalized_name1)
        .bind(&normalized_name2)
        .bind(&normalized_name2)
        .bind(&normalized_name1)
        .bind(entity_type.to_string())
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Check if two names are explicitly marked as not being aliases
    #[instrument(target = "db", level = "info", skip(self, name1, name2, entity_type))]
    pub async fn is_negative_match(
        &self,
        name1: &str,
        name2: &str,
        entity_type: &str,
    ) -> Result<bool, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) 
            FROM entity_negative_matches
            WHERE 
                ((normalized_name1 = ? AND normalized_name2 = ?)
                 OR (normalized_name1 = ? AND normalized_name2 = ?))
                AND entity_type = ?
            "#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name2)
        .bind(name1)
        .bind(entity_type)
        .fetch_one(self.pool())
        .await?;

        Ok(count > 0)
    }

    /// Update the statistics for a specific pattern type
    async fn increment_pattern_stat(
        &self,
        pattern_id: &str,
        approved: bool,
    ) -> Result<(), sqlx::Error> {
        let pattern_type = if pattern_id.starts_with("PATTERN_") {
            "REGEX"
        } else if pattern_id.starts_with("LLM_") {
            "LLM"
        } else {
            "OTHER"
        };

        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO alias_pattern_stats 
            (pattern_id, pattern_type, total_suggestions, approved_count, rejected_count, last_used_at)
            VALUES (?, ?, 1, ?, ?, ?)
            ON CONFLICT(pattern_id) DO UPDATE SET
                total_suggestions = total_suggestions + 1,
                approved_count = approved_count + ?,
                rejected_count = rejected_count + ?,
                last_used_at = excluded.last_used_at
            "#,
        )
        .bind(pattern_id)
        .bind(pattern_type)
        .bind(if approved { 1 } else { 0 })
        .bind(if !approved { 1 } else { 0 })
        .bind(now)
        .bind(if approved { 1 } else { 0 })
        .bind(if !approved { 1 } else { 0 })
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Migrate static aliases to the database
    pub async fn migrate_static_aliases(&self) -> Result<usize, sqlx::Error> {
        // Static aliases have been migrated to the database and
        // the static maps have been removed from the code.
        // This method is now deprecated but kept for backward compatibility.

        // Count how many static aliases are in the database
        let existing_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entity_aliases WHERE source = 'STATIC'")
                .fetch_one(self.pool())
                .await?;

        info!(target: TARGET_DB, "Found {} existing static aliases in the database", existing_count);

        // Just return the count of existing static aliases
        Ok(existing_count as usize)
    }

    /// Create a review batch for alias suggestions
    pub async fn create_alias_review_batch(&self, batch_size: i64) -> Result<i64, sqlx::Error> {
        let created_at = chrono::Utc::now().to_rfc3339();

        // Create the batch first
        let batch_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO alias_review_batches (created_at, status, total_count)
            VALUES (?, 'OPEN', 0)
            RETURNING id
            "#,
        )
        .bind(&created_at)
        .fetch_one(self.pool())
        .await?;

        // Get pending aliases to review
        let alias_ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id FROM entity_aliases
            WHERE status = 'PENDING'
            ORDER BY confidence DESC
            LIMIT ?
            "#,
        )
        .bind(batch_size)
        .fetch_all(self.pool())
        .await?;

        let total_count = alias_ids.len();

        // Add aliases to the batch
        for alias_id in alias_ids {
            sqlx::query(
                r#"
                INSERT INTO alias_review_items (batch_id, alias_id)
                VALUES (?, ?)
                "#,
            )
            .bind(batch_id)
            .bind(alias_id)
            .execute(self.pool())
            .await?;
        }

        // Update the batch with the actual count
        sqlx::query(
            r#"
            UPDATE alias_review_batches 
            SET total_count = ?
            WHERE id = ?
            "#,
        )
        .bind(total_count as i64)
        .bind(batch_id)
        .execute(self.pool())
        .await?;

        info!(target: TARGET_DB, "Created alias review batch #{} with {} items", batch_id, total_count);

        Ok(batch_id)
    }

    /// Get alias suggestions for a specific review batch
    pub async fn get_alias_review_batch(
        &self,
        batch_id: i64,
    ) -> Result<Vec<(i64, String, String, String, String, f64)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT 
                a.id,
                a.canonical_name,
                a.alias_text,
                a.entity_type,
                a.source,
                a.confidence
            FROM alias_review_items ri
            JOIN entity_aliases a ON ri.alias_id = a.id
            WHERE ri.batch_id = ? AND ri.decision IS NULL
            ORDER BY a.confidence DESC
            "#,
        )
        .bind(batch_id)
        .fetch_all(self.pool())
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("canonical_name"),
                    row.get("alias_text"),
                    row.get("entity_type"),
                    row.get("source"),
                    row.get::<f64, _>("confidence"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Approve an alias suggestion
    pub async fn approve_alias_suggestion(
        &self,
        alias_id: i64,
        admin_id: &str,
    ) -> Result<(), sqlx::Error> {
        // Get source to use as pattern_id before updating
        let source: Option<String> =
            sqlx::query_scalar("SELECT source FROM entity_aliases WHERE id = ?")
                .bind(alias_id)
                .fetch_optional(self.pool())
                .await?;

        let approved_at = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE entity_aliases
            SET status = 'APPROVED', approved_by = ?, approved_at = ?
            WHERE id = ?
            "#,
        )
        .bind(admin_id)
        .bind(&approved_at)
        .bind(alias_id)
        .execute(self.pool())
        .await?;

        // Update pattern stats if source is available
        if let Some(source_str) = source {
            // Fix: Convert Option<String> to String
            self.increment_pattern_stat(&source_str, true).await?;
        }

        info!(target: TARGET_DB, "Approved alias suggestion #{} by {}", alias_id, admin_id);

        Ok(())
    }

    /// Reject an alias suggestion
    pub async fn reject_alias_suggestion(
        &self,
        alias_id: i64,
        admin_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // Get the alias details before updating
        let row = sqlx::query(
            r#"
            SELECT canonical_name, alias_text, entity_type, entity_id, source
            FROM entity_aliases
            WHERE id = ?
            "#,
        )
        .bind(alias_id)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            let canonical_name: String = row.get("canonical_name");
            let alias_text: String = row.get("alias_text");
            let entity_type: String = row.get("entity_type");
            let entity_id: Option<i64> = row.get("entity_id");
            let source: Option<String> = row.get("source");

            // Update the alias status
            sqlx::query(
                r#"
                UPDATE entity_aliases
                SET status = 'REJECTED'
                WHERE id = ?
                "#,
            )
            .bind(alias_id)
            .execute(self.pool())
            .await?;

            // Update pattern stats if source is available
            if let Some(source_str) = source {
                // Fix: Convert Option<String> to String
                self.increment_pattern_stat(&source_str, false).await?;
            }

            // Optionally add to negative matches if reason is "different entity"
            if reason == Some("different entity") && entity_id.is_some() {
                let entity_type_enum = crate::entity::types::EntityType::from_str(&entity_type)
                    .map_err(|e| {
                        sqlx::Error::Protocol(format!("Invalid entity type: {}", e).into())
                    })?;

                self.add_negative_match(
                    entity_id.unwrap(),
                    &canonical_name,
                    &alias_text,
                    entity_type_enum,
                    admin_id,
                )
                .await?;
            }

            info!(target: TARGET_DB, "Rejected alias suggestion #{} by {}: {} ↔ {} ({})", 
                 alias_id, admin_id, canonical_name, alias_text, entity_type);
        }

        Ok(())
    }

    /// Get statistics about the alias system
    pub async fn get_alias_system_stats(&self) -> Result<serde_json::Value, sqlx::Error> {
        // Get overall counts
        let total_approved: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'APPROVED'"#)
                .fetch_one(self.pool())
                .await?;

        let total_rejected: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'REJECTED'"#)
                .fetch_one(self.pool())
                .await?;

        let total_pending: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_aliases WHERE status = 'PENDING'"#)
                .fetch_one(self.pool())
                .await?;

        let negative_matches: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM entity_negative_matches"#)
                .fetch_one(self.pool())
                .await?;

        // Get counts by source
        let source_rows = sqlx::query(
            r#"
            SELECT source, COUNT(*) as count
            FROM entity_aliases
            GROUP BY source
            ORDER BY count DESC
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        let by_source: serde_json::Map<String, serde_json::Value> = source_rows
            .into_iter()
            .map(|row| {
                let source: String = row.get("source");
                let count: i64 = row.get("count");
                (source, serde_json::Value::from(count))
            })
            .collect();

        // Get pattern statistics
        let pattern_rows = sqlx::query(
            r#"
            SELECT 
                pattern_id,
                pattern_type,
                total_suggestions,
                approved_count,
                rejected_count,
                enabled
            FROM alias_pattern_stats
            ORDER BY total_suggestions DESC
            LIMIT 20
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        let pattern_stats: Vec<serde_json::Value> = pattern_rows
            .into_iter()
            .map(|row| {
                let mut pattern_data = serde_json::Map::new();
                pattern_data.insert(
                    "pattern_id".to_string(),
                    serde_json::Value::from(row.get::<String, _>("pattern_id")),
                );
                pattern_data.insert(
                    "pattern_type".to_string(),
                    serde_json::Value::from(row.get::<String, _>("pattern_type")),
                );
                pattern_data.insert(
                    "total".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("total_suggestions")),
                );
                pattern_data.insert(
                    "approved".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("approved_count")),
                );
                pattern_data.insert(
                    "rejected".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("rejected_count")),
                );
                pattern_data.insert(
                    "enabled".to_string(),
                    serde_json::Value::from(row.get::<bool, _>("enabled")),
                );

                serde_json::Value::Object(pattern_data)
            })
            .collect();

        // Get top rejected pairs
        let rejected_rows = sqlx::query(
            r#"
            SELECT 
                normalized_name1,
                normalized_name2,
                entity_type,
                COUNT(*) as rejection_count
            FROM entity_negative_matches
            GROUP BY normalized_name1, normalized_name2, entity_type
            ORDER BY rejection_count DESC
            LIMIT 10
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        let top_rejected_pairs: Vec<serde_json::Value> = rejected_rows
            .into_iter()
            .map(|row| {
                let mut pair_data = serde_json::Map::new();
                pair_data.insert(
                    "name1".to_string(),
                    serde_json::Value::from(row.get::<String, _>("normalized_name1")),
                );
                pair_data.insert(
                    "name2".to_string(),
                    serde_json::Value::from(row.get::<String, _>("normalized_name2")),
                );
                pair_data.insert(
                    "entity_type".to_string(),
                    serde_json::Value::from(row.get::<String, _>("entity_type")),
                );
                pair_data.insert(
                    "rejection_count".to_string(),
                    serde_json::Value::from(row.get::<i64, _>("rejection_count")),
                );

                serde_json::Value::Object(pair_data)
            })
            .collect();

        // Build the final JSON response
        let mut stats = serde_json::Map::new();
        stats.insert(
            "total_approved".to_string(),
            serde_json::Value::from(total_approved),
        );
        stats.insert(
            "total_rejected".to_string(),
            serde_json::Value::from(total_rejected),
        );
        stats.insert(
            "total_pending".to_string(),
            serde_json::Value::from(total_pending),
        );
        stats.insert(
            "negative_matches".to_string(),
            serde_json::Value::from(negative_matches),
        );
        stats.insert(
            "by_source".to_string(),
            serde_json::Value::Object(by_source),
        );
        stats.insert(
            "pattern_stats".to_string(),
            serde_json::Value::Array(pattern_stats),
        );
        stats.insert(
            "top_rejected_pairs".to_string(),
            serde_json::Value::Array(top_rejected_pairs),
        );

        Ok(serde_json::Value::Object(stats))
    }
}
