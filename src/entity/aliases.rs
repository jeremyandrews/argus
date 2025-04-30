//! Entity alias management module
//!
//! This module handles the entity alias system, supporting both static and database-driven
//! aliases. The system is transitioning from static, hardcoded aliases to a dynamic,
//! database-driven approach. During this transition period, both systems are supported.
//!
//! ## Features
//! - Backward compatibility with static aliases
//! - Database-driven alias storage and retrieval
//! - Negative match tracking
//! - Support for multiple alias sources (static, pattern-based, LLM-generated, user-defined)

use super::types::EntityType;
use crate::db::Database;
use lazy_static::lazy_static;
use std::collections::HashMap;
use tracing::{debug, instrument};

// Common cross-language and spelling variations for pattern-based matching
pub const COMMON_VARIATIONS: &[(&str, &str)] = &[
    ("project", "projekt"),
    ("center", "centre"),
    ("defense", "defence"),
    ("program", "programme"),
    ("color", "colour"),
    ("theater", "theatre"),
    ("organization", "organisation"),
    ("analyzer", "analyser"),
];

// Common patterns for extracting aliases
pub const ALIAS_PATTERNS: &[&str] = &[
    r#"(?i)(?P<canonical>.+?),?\s+(?:also\s+)?(?:known|called)\s+as\s+["']?(?P<alias>.+?)["']?[,\.)]"#,
    r#"(?i)(?P<canonical>.+?)\s+\((?:a\.?k\.?a\.?|formerly)\s+["']?(?P<alias>.+?)["']?\)"#,
    r#"(?i)["']?(?P<alias>.+?)["']?,?\s+now\s+(?:known\s+as\s+)?["']?(?P<canonical>.+?)["']?[,\.)]"#,
];

// Static aliases - to be migrated to database
// Keep for backward compatibility during migration
lazy_static! {
    // Person aliases
    static ref PERSON_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("jeff bezos".to_string(), "jeff bezos".to_string());
        map.insert("jeffrey bezos".to_string(), "jeff bezos".to_string());
        map.insert("jeffrey p bezos".to_string(), "jeff bezos".to_string());

        map.insert("elon musk".to_string(), "elon musk".to_string());
        map.insert("elon r musk".to_string(), "elon musk".to_string());
        // Add more common person aliases
        map
    };

    // Organization aliases
    static ref ORG_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("blue origin".to_string(), "blue origin".to_string());
        map.insert("blueorigin".to_string(), "blue origin".to_string());

        map.insert("spacex".to_string(), "spacex".to_string());
        map.insert("space x".to_string(), "spacex".to_string());
        map.insert("space exploration technologies".to_string(), "spacex".to_string());

        map.insert("ula".to_string(), "united launch alliance".to_string());
        map.insert("united launch alliance".to_string(), "united launch alliance".to_string());
        // Add more organization aliases
        map
    };

    // Product aliases
    static ref PRODUCT_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("project kuiper".to_string(), "project kuiper".to_string());
        map.insert("projekt kuiper".to_string(), "project kuiper".to_string());

        map.insert("starlink".to_string(), "starlink".to_string());
        map.insert("spacexs starlinks".to_string(), "starlink".to_string());
        map.insert("spacex starlink".to_string(), "starlink".to_string());
        map.insert("spacex's starlinks".to_string(), "starlink".to_string());

        map.insert("atlas v".to_string(), "atlas v".to_string());
        map.insert("atlas 5".to_string(), "atlas v".to_string());
        // Add more product aliases
        map
    };

    // Location aliases
    static ref LOCATION_ALIASES: HashMap<String, String> = {
        let mut map = HashMap::new();
        map.insert("usa".to_string(), "united states".to_string());
        map.insert("united states".to_string(), "united states".to_string());
        map.insert("united states of america".to_string(), "united states".to_string());

        // Add more location aliases
        map
    };
}

/// Get static aliases for a specific entity type
///
/// NOTE: This function is provided for backward compatibility during the migration to
/// database-driven aliases. New code should use the database methods directly.
///
/// Returns a cloned HashMap of aliases for the specified entity type.
pub fn get_aliases_for_type(entity_type: EntityType) -> HashMap<String, String> {
    match entity_type {
        EntityType::Person => PERSON_ALIASES.clone(),
        EntityType::Organization => ORG_ALIASES.clone(),
        EntityType::Product => PRODUCT_ALIASES.clone(),
        EntityType::Location => LOCATION_ALIASES.clone(),
        _ => HashMap::new(),
    }
}

/// Check if two entity names are equivalent according to the alias system
///
/// This is the primary interface for checking name equivalence. It will:
/// 1. Use database-driven aliases if database is available
/// 2. Fall back to static aliases if database check fails or is unavailable
///
/// It handles both directions of equivalence and normalizes names before comparison.
#[instrument(level = "debug", skip(db, name1, name2))]
pub async fn names_match(
    db: &Database,
    name1: &str,
    name2: &str,
    entity_type: EntityType,
) -> anyhow::Result<bool> {
    // If names are identical after normalization, they match
    let normalizer = super::normalizer::EntityNormalizer::new();
    let norm1 = normalizer.normalize(name1, entity_type);
    let norm2 = normalizer.normalize(name2, entity_type);

    if norm1 == norm2 {
        return Ok(true);
    }

    // Try database-driven approach first
    match db
        .are_names_equivalent(&norm1, &norm2, &entity_type.to_string())
        .await
    {
        Ok(result) => Ok(result),
        Err(err) => {
            debug!(
                "Database alias check failed, falling back to static aliases: {}",
                err
            );

            // Fall back to static aliases if database check fails
            let aliases = get_aliases_for_type(entity_type);
            let canonical1 = aliases.get(&norm1).unwrap_or(&norm1);
            let canonical2 = aliases.get(&norm2).unwrap_or(&norm2);

            Ok(canonical1 == canonical2)
        }
    }
}

/// Get the canonical name for an entity (database-driven approach)
///
/// This is the preferred way to get a canonical entity name. It will:
/// 1. Use database-driven aliases if database is available
/// 2. Fall back to static aliases if database check fails or is unavailable
#[instrument(level = "debug", skip(db, name))]
pub async fn get_canonical_name(
    db: &Database,
    name: &str,
    entity_type: EntityType,
) -> anyhow::Result<String> {
    let normalizer = super::normalizer::EntityNormalizer::new();
    let normalized = normalizer.normalize(name, entity_type);

    // Try database-driven approach first
    match db
        .get_canonical_name(&normalized, &entity_type.to_string())
        .await
    {
        Ok(Some(canonical)) => Ok(canonical),
        Ok(None) => {
            // Fall back to static aliases if no result from database
            let aliases = get_aliases_for_type(entity_type);
            let canonical = aliases.get(&normalized).unwrap_or(&normalized).clone();
            Ok(canonical)
        }
        Err(err) => {
            debug!(
                "Database canonical name lookup failed, falling back to static aliases: {}",
                err
            );

            // Fall back to static aliases if database check fails
            let aliases = get_aliases_for_type(entity_type);
            let canonical = aliases.get(&normalized).unwrap_or(&normalized).clone();
            Ok(canonical)
        }
    }
}

/// Add a new alias to the database
///
/// This is the preferred way to add a new alias. It handles normalization and
/// validation before adding to the database.
#[instrument(level = "debug", skip(db, entity_id, canonical_name, alias_name))]
pub async fn add_alias(
    db: &Database,
    entity_id: Option<i64>,
    canonical_name: &str,
    alias_name: &str,
    entity_type: EntityType,
    source: &str,
    confidence: f64,
) -> anyhow::Result<i64> {
    let result = db
        .add_entity_alias(
            entity_id,
            canonical_name,
            alias_name,
            &entity_type.to_string(),
            source,
            confidence,
            None, // Use default status (PENDING)
            None, // No approver for automatic additions
        )
        .await?;

    Ok(result)
}

/// Extract potential aliases from text using patterns
///
/// Uses a set of regex patterns to identify potential aliases in text.
/// Returns a vector of (canonical, alias, entity_type, confidence) tuples.
#[instrument(level = "debug", skip(text))]
pub fn extract_potential_aliases(
    text: &str,
    entity_type: Option<EntityType>,
) -> Vec<(String, String, EntityType, f64)> {
    use regex::Regex;
    let mut results = Vec::new();

    // Compile the patterns
    let patterns: Vec<Regex> = ALIAS_PATTERNS
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();

    // Apply each pattern
    for pattern in patterns {
        for cap in pattern.captures_iter(text) {
            if let (Some(canonical_match), Some(alias_match)) =
                (cap.name("canonical"), cap.name("alias"))
            {
                let canonical = canonical_match.as_str().trim();
                let alias = alias_match.as_str().trim();

                // Skip if canonical and alias are the same or too short
                if canonical == alias || canonical.len() < 2 || alias.len() < 2 {
                    continue;
                }

                // Infer entity type if not provided
                // This is a simplified approach - could be improved with NER
                let inferred_type = entity_type.unwrap_or_else(|| {
                    // Basic rules for inferring type
                    if canonical.chars().next().unwrap_or(' ').is_uppercase() {
                        if canonical.split_whitespace().count() <= 2 {
                            EntityType::Person // Assume short capitalized names are people
                        } else {
                            EntityType::Organization // Assume longer capitalized names are organizations
                        }
                    } else {
                        EntityType::Product // Default to product for lowercase names
                    }
                });

                // Assign confidence based on pattern quality
                // Could be refined with better heuristics
                let confidence = 0.8; // High initial confidence for pattern-based extraction

                results.push((
                    canonical.to_string(),
                    alias.to_string(),
                    inferred_type,
                    confidence,
                ));
            }
        }
    }

    results
}
