use crate::entity::types::{Entity, EntityType, ExtractedEntities, ImportanceLevel};
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::LLMParams;
use crate::WorkerDetail;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info};

use super::TARGET_ENTITY;

/// Extract entities from article text using LLM
pub async fn extract_entities(
    article_text: &str,
    pub_date: Option<&str>,
    llm_params: &mut LLMParams,
    worker_detail: &WorkerDetail,
) -> Result<ExtractedEntities> {
    // Set up extraction prompt
    let entity_prompt = prompts::entity_extraction_prompt(article_text, pub_date);

    // Enable structured JSON output mode with entity extraction schema
    llm_params.json_format = Some(crate::JsonSchemaType::EntityExtraction);

    // Get LLM response
    let response = match generate_llm_response(&entity_prompt, llm_params, worker_detail).await {
        Some(response) => response,
        None => {
            error!(target: TARGET_ENTITY, "Failed to generate entity extraction response");
            return Err(anyhow::anyhow!(
                "Entity extraction failed: No response from LLM"
            ));
        }
    };

    // Log the raw response for debugging
    info!(target: TARGET_ENTITY, "Raw LLM response for entity extraction: {}", response);

    // Reset JSON format mode
    llm_params.json_format = None;

    // Parse the response
    let parsed = match parse_entity_response(&response) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!(
                target: TARGET_ENTITY,
                "Failed to parse entity response: {}. Raw response: {}",
                e, response
            );
            return Err(anyhow::anyhow!("Entity extraction failed: {}", e));
        }
    };

    info!(
        target: TARGET_ENTITY,
        "Successfully extracted {} entities from article text",
        parsed.entities.len()
    );

    Ok(parsed)
}

/// Parse and normalize entity extraction response
fn parse_entity_response(json_str: &str) -> Result<ExtractedEntities> {
    // First try to parse the JSON
    let json: Value = match serde_json::from_str(json_str) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!(
                target: TARGET_ENTITY,
                "Failed to parse JSON: {}. Raw content: {}",
                e, &json_str[..std::cmp::min(500, json_str.len())] // Show up to 500 chars to avoid giant logs
            );
            return Err(anyhow::anyhow!("Invalid JSON response: {}", e));
        }
    };

    // Log the top-level structure for debugging
    info!(
        target: TARGET_ENTITY,
        "Top-level JSON structure: {}",
        json.as_object()
            .map(|obj| obj.keys().map(|k| k.to_string()).collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| "Not an object".to_string())
    );

    // Create new entity collection
    let mut extracted = ExtractedEntities::new();

    // Extract event date if present
    if let Some(date) = json.get("event_date").and_then(Value::as_str) {
        if !date.is_empty() {
            extracted.event_date = Some(date.to_string());
            debug!(
                target: TARGET_ENTITY,
                "Found event date: {}", date
            );
        }
    }

    // Extract entities
    match json.get("entities") {
        Some(entities_value) => match entities_value.as_array() {
            Some(entities) => {
                for entity_value in entities {
                    if let Some(entity) = parse_entity_object(entity_value) {
                        extracted.add_entity(entity);
                    }
                }
            }
            None => {
                let type_str = if entities_value.is_object() {
                    "object"
                } else if entities_value.is_string() {
                    "string"
                } else if entities_value.is_number() {
                    "number"
                } else if entities_value.is_boolean() {
                    "boolean"
                } else if entities_value.is_null() {
                    "null"
                } else {
                    "unknown"
                };

                error!(
                    target: TARGET_ENTITY,
                    "The 'entities' field is not an array. Actual type: {}, Value: {}",
                    type_str, entities_value
                );
                return Err(anyhow::anyhow!("The 'entities' field is not an array"));
            }
        },
        None => {
            error!(
                target: TARGET_ENTITY,
                "No 'entities' field found in response. Top-level fields: {}",
                json.as_object()
                    .map(|obj| obj.keys().map(|k| k.to_string()).collect::<Vec<_>>().join(", "))
                    .unwrap_or_else(|| "None".to_string())
            );
            return Err(anyhow::anyhow!("No entities array in extraction response"));
        }
    }

    debug!(
        target: TARGET_ENTITY,
        "Parsed {} entities from extraction response",
        extracted.entities.len()
    );

    Ok(extracted)
}

/// Parse individual entity from JSON object
fn parse_entity_object(entity_value: &Value) -> Option<Entity> {
    let name = entity_value.get("name")?.as_str()?;

    // Extract normalized name or generate from name
    let normalized_name = entity_value
        .get("normalized_name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| name)
        .to_lowercase();

    // Extract entity type or default to Other
    let type_str = entity_value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("OTHER");
    let entity_type = EntityType::from(type_str);

    // Extract importance or default to Mentioned
    let importance_str = entity_value
        .get("importance")
        .and_then(Value::as_str)
        .unwrap_or("MENTIONED");
    let importance = ImportanceLevel::from(importance_str);

    // Create basic entity
    let mut entity = Entity::new(name, &normalized_name, entity_type, importance);

    // Extract metadata if any
    if let Some(obj) = entity_value.as_object() {
        let mut metadata = HashMap::new();

        // Add any additional fields as metadata
        for (key, value) in obj {
            if !["name", "normalized_name", "type", "importance"].contains(&key.as_str()) {
                if let Some(value_str) = value.as_str() {
                    metadata.insert(key.clone(), value_str.to_string());
                }
            }
        }

        if !metadata.is_empty() {
            entity = entity.with_metadata(metadata);
        }
    }

    Some(entity)
}

/// Normalize entity name for consistent matching
pub fn normalize_entity_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .trim()
        .to_string()
}
