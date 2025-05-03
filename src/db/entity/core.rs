use crate::db::core::Database;
use sqlx::Row;

impl Database {
    /// Add a new entity to the database or return existing entity ID if it already exists
    pub async fn add_entity(
        &self,
        name: &str,
        entity_type: &str,
        normalized_name: &str,
        parent_id: Option<i64>,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO entities (name, type, normalized_name, parent_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(normalized_name, type) DO UPDATE SET
                name = excluded.name,
                parent_id = excluded.parent_id
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(entity_type)
        .bind(normalized_name)
        .bind(parent_id)
        .fetch_one(self.pool())
        .await?;

        Ok(result.get("id"))
    }

    /// Get detail information about a specific entity
    pub async fn get_entity_details(
        &self,
        entity_id: i64,
    ) -> Result<Option<(String, String, Option<i64>)>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT name, type, parent_id
            FROM entities
            WHERE id = ?1
            "#,
        )
        .bind(entity_id)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            Ok(Some((
                row.get("name"),
                row.get("type"),
                row.get("parent_id"),
            )))
        } else {
            Ok(None)
        }
    }

    /// Process entity extraction JSON from LLM and add entities to an article
    pub async fn process_entity_extraction(
        &self,
        article_id: i64,
        extraction_json: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let entities: serde_json::Value = serde_json::from_str(extraction_json)
            .map_err(|e| sqlx::Error::Protocol(format!("Invalid JSON: {}", e).into()))?;

        let mut added_entity_ids = Vec::new();

        if let Some(entities_array) = entities["entities"].as_array() {
            for entity in entities_array {
                // Extract entity data
                let name = entity["name"]
                    .as_str()
                    .ok_or_else(|| sqlx::Error::Protocol("Missing entity name".into()))?;
                let entity_type = entity["entity_type"]
                    .as_str()
                    .ok_or_else(|| sqlx::Error::Protocol("Missing entity type".into()))?;
                // Create a separate variable for the lowercase name to extend its lifetime
                let lowercase_name = name.to_lowercase();
                let normalized_name = entity["normalized_name"]
                    .as_str()
                    .unwrap_or(lowercase_name.as_str());
                let importance = entity["importance"].as_str().unwrap_or("MENTIONED");

                // Add entity to database
                let entity_id = self
                    .add_entity(name, entity_type, normalized_name, None)
                    .await?;

                // Add entity to article with importance
                self.add_entity_to_article(article_id, entity_id, importance, None)
                    .await?;

                added_entity_ids.push(entity_id);
            }
        }

        // If event_date is present in the extraction, update the article
        if let Some(event_date) = entities["event_date"].as_str() {
            if !event_date.is_empty() {
                sqlx::query(
                    r#"
                    UPDATE articles
                    SET event_date = ?1
                    WHERE id = ?2
                    "#,
                )
                .bind(event_date)
                .bind(article_id)
                .execute(self.pool())
                .await?;
            }
        }

        Ok(added_entity_ids)
    }

    /// Find articles that have associated entities and were published after a certain date
    pub async fn find_articles_with_entities(
        &self,
        date_threshold: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query_scalar(
            r#"
            SELECT DISTINCT a.id
            FROM articles a
            JOIN article_entities ae ON a.id = ae.article_id
            WHERE (a.pub_date >= ?1 OR a.pub_date IS NULL)
            ORDER BY a.id DESC
            LIMIT 5000
            "#,
        )
        .bind(date_threshold)
        .fetch_all(self.pool())
        .await?;

        Ok(rows)
    }
}
