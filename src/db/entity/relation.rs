use serde_json;
use sqlx::Row;
use tracing::{error, info};

use crate::db::core::Database;
use crate::TARGET_DB;

impl Database {
    /// Link an entity to an article with specified importance
    pub async fn add_entity_to_article(
        &self,
        article_id: i64,
        entity_id: i64,
        importance: &str,
        context: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO article_entities (article_id, entity_id, importance, context)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(article_id, entity_id) DO UPDATE SET
                importance = excluded.importance,
                context = excluded.context
            "#,
        )
        .bind(article_id)
        .bind(entity_id)
        .bind(importance)
        .bind(context)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Get all entities for a specific article
    pub async fn get_article_entities(
        &self,
        article_id: i64,
    ) -> Result<Vec<(i64, String, String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT e.id, e.name, e.type, ae.importance
            FROM entities e
            JOIN article_entities ae ON e.id = ae.entity_id
            WHERE ae.article_id = ?1
            ORDER BY ae.importance, e.type, e.name
            "#,
        )
        .bind(article_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("name"),
                    row.get("type"),
                    row.get("importance"),
                )
            })
            .collect())
    }

    /// Get articles that share significant entities with the given entity IDs
    pub async fn get_articles_by_entities(
        &self,
        entity_ids: &[i64],
        limit: u64,
    ) -> Result<Vec<(i64, Option<String>, Option<String>, i64, i64)>, sqlx::Error> {
        // Convert entity_ids to a JSON array string for SQLite's json_each function
        let entity_ids_json = serde_json::to_string(entity_ids).map_err(|e| {
            sqlx::Error::Protocol(format!("JSON serialization error: {}", e).into())
        })?;

        // Query for articles that share entities, prioritizing those with PRIMARY importance
        let rows = sqlx::query(
            r#"
            SELECT a.id, a.pub_date as published_date, a.category, a.quality_score,
                   COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                   COUNT(ae.entity_id) as total_count
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
        )
        .bind(&entity_ids_json)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await?;

        // Convert rows to tuples - must match the expected return type in function signature
        let results: Vec<(i64, Option<String>, Option<String>, i64, i64)> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get::<i64, _>("primary_count"),
                    row.get::<i64, _>("total_count"),
                )
            })
            .collect();

        Ok(results)
    }

    /// Get articles that share significant entities with the given entity IDs
    /// and are within a date window around the source article date
    pub async fn get_articles_by_entities_with_date(
        &self,
        entity_ids: &[i64],
        limit: u64,
        source_date: &str,
    ) -> Result<Vec<(i64, Option<String>, Option<String>, i64, i64)>, sqlx::Error> {
        // Log the search criteria
        info!(target: TARGET_DB, "Looking for articles with entities: {:?}, source date: {}, limit: {}", 
              entity_ids, source_date, limit);

        // Convert entity_ids to a JSON array string for SQLite's json_each function
        let entity_ids_json = serde_json::to_string(entity_ids).map_err(|e| {
            sqlx::Error::Protocol(format!("JSON serialization error: {}", e).into())
        })?;

        // First, try a query WITHOUT the date filter to see if we have ANY matching articles
        let check_query = r#"
            SELECT COUNT(*) 
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
            "#;

        let _total_matching: i64 = match sqlx::query_scalar(check_query)
            .bind(&entity_ids_json)
            .fetch_one(self.pool())
            .await
        {
            Ok(count) => {
                info!(target: TARGET_DB, "Found {} total articles that share entities with: {:?} (without date filter)", 
                    count, entity_ids);
                count
            }
            Err(e) => {
                error!(target: TARGET_DB, "Failed to check total matching articles: {}", e);
                0
            }
        };

        // Base query without date filtering
        let base_query = r#"
            SELECT a.id, a.pub_date as published_date, a.category,
                   COUNT(CASE WHEN ae.importance = 'PRIMARY' THEN 1 ELSE NULL END) as primary_count,
                   COUNT(ae.entity_id) as total_count
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE ae.entity_id IN (SELECT value FROM json_each(?))
        "#;

        // Query with date filtering using COALESCE to check both event_date and pub_date
        // and using today's date as the reference point for the window
        let query_with_date_filter = format!(
            r#"
            {}
            AND COALESCE(
                date(substr(a.event_date, 1, 10)),
                date(substr(a.pub_date, 1, 10))
            ) BETWEEN date('now', '-14 days')
                   AND date('now', '+1 day')
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
            base_query
        );

        // Query without date filtering
        let query_without_date_filter = format!(
            r#"
            {}
            GROUP BY a.id
            ORDER BY primary_count DESC, total_count DESC
            LIMIT ?
            "#,
            base_query
        );

        // Log the date window we're using
        info!(target: TARGET_DB, "Using date window: from today - 14 days to today + 1 day");

        // Decide which query to use and execute it
        let rows = if !source_date.is_empty() {
            info!(target: TARGET_DB, "Using query with date filtering");
            sqlx::query(&query_with_date_filter)
                .bind(&entity_ids_json)
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
        } else {
            info!(target: TARGET_DB, "No source date provided, skipping date filtering");
            sqlx::query(&query_without_date_filter)
                .bind(&entity_ids_json)
                .bind(limit as i64)
                .fetch_all(self.pool())
                .await?
        };

        info!(target: TARGET_DB, "Entity search returned {} results for entities: {:?} using date window", 
            rows.len(), entity_ids);

        // Convert rows to tuples - must match the expected return type (5-tuple)
        let results: Vec<(i64, Option<String>, Option<String>, i64, i64)> = rows
            .into_iter()
            .map(|row| {
                (
                    row.get("id"),
                    row.get("published_date"),
                    row.get("category"),
                    row.get::<i64, _>("primary_count"),
                    row.get::<i64, _>("total_count"),
                )
            })
            .collect();

        Ok(results)
    }
}
