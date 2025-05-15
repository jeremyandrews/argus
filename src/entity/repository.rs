use crate::db::core::Database;
use crate::entity::types::{Entity, EntityType, ExtractedEntities, ImportanceLevel};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info};

use super::TARGET_ENTITY;

/// Store all extracted entities for an article
pub async fn store_entities(
    db: &Database,
    article_id: i64,
    entities: &ExtractedEntities,
) -> Result<Vec<i64>> {
    let mut entity_ids = Vec::new();

    // Update article with event date if present
    if let Some(event_date) = &entities.event_date {
        update_article_event_date(db, article_id, event_date).await?;
    }

    // Store all entities
    for entity in &entities.entities {
        match store_entity(db, article_id, entity).await {
            Ok(id) => {
                entity_ids.push(id);
            }
            Err(e) => {
                error!(
                    target: TARGET_ENTITY,
                    "Failed to store entity {}: {}", entity.name, e
                );
                // Continue with other entities even if one fails
            }
        }
    }

    info!(
        target: TARGET_ENTITY,
        "Stored {} entities for article {}", entity_ids.len(), article_id
    );

    Ok(entity_ids)
}

/// Store a single entity and link it to the article
async fn store_entity(db: &Database, article_id: i64, entity: &Entity) -> Result<i64> {
    // Add the entity to the database
    let entity_id = db
        .add_entity(
            &entity.name,
            &entity.entity_type.to_string(),
            &entity.normalized_name,
            None, // No parent ID for now
        )
        .await
        .context("Failed to add entity to database")?;

    // Add the entity-article relationship
    let importance = entity.importance.to_string();

    // Convert metadata to JSON string if present
    let context = if let Some(metadata) = &entity.metadata {
        Some(serde_json::to_string(metadata)?)
    } else {
        None
    };

    // Link entity to article
    db.add_entity_to_article(article_id, entity_id, &importance, context.as_deref())
        .await
        .context("Failed to link entity to article")?;

    debug!(
        target: TARGET_ENTITY,
        "Stored entity '{}' with ID {} for article {}", entity.name, entity_id, article_id
    );

    Ok(entity_id)
}

/// Update the article's event_date field
async fn update_article_event_date(db: &Database, article_id: i64, event_date: &str) -> Result<()> {
    // Query to update just the event_date field
    sqlx::query(
        r#"
        UPDATE articles
        SET event_date = ?1
        WHERE id = ?2
        "#,
    )
    .bind(event_date)
    .bind(article_id)
    .execute(db.pool())
    .await
    .context("Failed to update article event_date")?;

    debug!(
        target: TARGET_ENTITY,
        "Updated event_date to '{}' for article {}", event_date, article_id
    );

    Ok(())
}

/// Get all entities for an article
pub async fn get_article_entities(db: &Database, article_id: i64) -> Result<ExtractedEntities> {
    info!(
        target: TARGET_ENTITY,
        "Retrieving entities for article {}", article_id
    );

    // Get the article's event_date
    let article_date = db
        .get_article_details_with_dates(article_id)
        .await
        .context("Failed to get article dates")?;

    // Build ExtractedEntities object
    let mut extracted = ExtractedEntities::new();

    // Set event date if available
    if let (_, Some(event_date)) = article_date {
        extracted.event_date = Some(event_date.clone());
        info!(
            target: TARGET_ENTITY,
            "Found event_date '{}' for article {}", event_date, article_id
        );
    }

    // Get all entities linked to this article
    let entities = db
        .get_article_entities(article_id)
        .await
        .context("Failed to get article entities")?;

    info!(
        target: TARGET_ENTITY,
        "Database returned {} entities for article {}", entities.len(), article_id
    );

    // Convert database rows to Entity objects
    for (entity_id, name, entity_type_str, importance_str) in entities {
        let entity_type = EntityType::from(entity_type_str.as_str());
        let importance = ImportanceLevel::from(importance_str.as_str());

        debug!(
            target: TARGET_ENTITY,
            "Adding entity: id={}, name='{}', type={}, importance={}",
            entity_id, name, entity_type_str, importance_str
        );

        // Create entity without metadata first
        let entity =
            Entity::new(&name, &name.to_lowercase(), entity_type, importance).with_id(entity_id);

        // Add to collection
        extracted.add_entity(entity);
    }

    let entity_types_count = extracted.entities.iter().fold(
        std::collections::HashMap::<EntityType, usize>::new(),
        |mut acc, e| {
            *acc.entry(e.entity_type).or_insert(0) += 1;
            acc
        },
    );

    info!(
        target: TARGET_ENTITY,
        "Retrieved {} entities for article {} - Types: {:?}",
        extracted.entities.len(),
        article_id,
        entity_types_count
    );

    Ok(extracted)
}

/// Extract and process entities from LLM JSON response
pub async fn process_entity_extraction(
    db: &Database,
    article_id: i64,
    json_str: &str,
) -> Result<Vec<i64>> {
    info!(
        target: TARGET_ENTITY,
        "Processing entity extraction for article {}", article_id
    );

    // Parse the JSON response
    let parsed_entities =
        parse_entity_json(json_str).context("Failed to parse entity extraction JSON")?;

    // Store the entities and their relationships
    store_entities(db, article_id, &parsed_entities).await
}

/// Parse entity extraction JSON into structured entity objects
pub fn parse_entity_json(json_str: &str) -> Result<ExtractedEntities> {
    // Parse JSON
    let json: Value = serde_json::from_str(json_str)
        .context("Invalid JSON format in entity extraction response")?;

    // Create new entity collection
    let mut extracted = ExtractedEntities::new();

    // Extract event_date if present
    if let Some(date) = json.get("event_date").and_then(|d| d.as_str()) {
        if !date.is_empty() {
            extracted.event_date = Some(date.to_string());
            debug!(
                target: TARGET_ENTITY,
                "Found event date: {}", date
            );
        }
    }

    // Extract entities
    if let Some(entities) = json.get("entities").and_then(|e| e.as_array()) {
        for entity_json in entities {
            if let (Some(name), Some(entity_type), Some(importance)) = (
                entity_json.get("name").and_then(|n| n.as_str()),
                entity_json.get("type").and_then(|t| t.as_str()),
                entity_json.get("importance").and_then(|i| i.as_str()),
            ) {
                // Get normalized name or create from name
                let normalized_name = entity_json
                    .get("normalized_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_else(|| name)
                    .to_lowercase();

                // Create entity
                let entity_type = EntityType::from(entity_type);
                let importance = ImportanceLevel::from(importance);
                let mut entity = Entity::new(name, &normalized_name, entity_type, importance);

                // Add metadata if present
                if let Some(obj) = entity_json.as_object() {
                    let mut metadata = HashMap::new();
                    for (key, value) in obj {
                        if !["name", "normalized_name", "type", "importance"]
                            .contains(&key.as_str())
                        {
                            if let Some(value_str) = value.as_str() {
                                metadata.insert(key.clone(), value_str.to_string());
                            }
                        }
                    }
                    if !metadata.is_empty() {
                        entity = entity.with_metadata(metadata);
                    }
                }

                // Add to collection
                extracted.add_entity(entity);
            }
        }
    }

    debug!(
        target: TARGET_ENTITY,
        "Parsed {} entities from extraction JSON",
        extracted.entities.len()
    );

    Ok(extracted)
}
