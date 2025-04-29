use crate::entity::types::{
    EntitySimilarityMetrics, EntityType, ExtractedEntities, ImportanceLevel,
};
use chrono::NaiveDate;
use std::collections::HashSet;
use tracing::{debug, error, info, warn};

use super::normalizer::EntityNormalizer;
use super::TARGET_ENTITY;

/// Calculate similarity between articles based on entity overlap
pub fn calculate_entity_similarity(
    source_entities: &ExtractedEntities,
    target_entities: &ExtractedEntities,
    source_date: Option<&str>,
    target_date: Option<&str>,
) -> EntitySimilarityMetrics {
    // Create normalizer for entity comparisons
    let normalizer = EntityNormalizer::new();

    // Log detailed information about the entities we're comparing
    info!(
        target: TARGET_ENTITY,
        "Calculating entity similarity between source ({} entities) and target ({} entities)",
        source_entities.entities.len(), target_entities.entities.len()
    );

    let mut metrics = EntitySimilarityMetrics::new();

    // 1. Calculate basic entity overlap
    metrics.entity_overlap_count =
        count_entity_overlap(source_entities, target_entities, &normalizer);

    // Log entity type breakdown for source and target
    info!(
        target: TARGET_ENTITY,
        "Source entity types: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
        source_entities.get_entities_by_type(EntityType::Person).len(),
        source_entities.get_entities_by_type(EntityType::Organization).len(),
        source_entities.get_entities_by_type(EntityType::Location).len(),
        source_entities.get_entities_by_type(EntityType::Event).len()
    );

    info!(
        target: TARGET_ENTITY,
        "Target entity types: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
        target_entities.get_entities_by_type(EntityType::Person).len(),
        target_entities.get_entities_by_type(EntityType::Organization).len(),
        target_entities.get_entities_by_type(EntityType::Location).len(),
        target_entities.get_entities_by_type(EntityType::Event).len()
    );

    // 2. Calculate type-specific similarity scores
    metrics.person_overlap = calculate_type_similarity(
        source_entities,
        target_entities,
        EntityType::Person,
        &normalizer,
    );

    metrics.organization_overlap = calculate_type_similarity(
        source_entities,
        target_entities,
        EntityType::Organization,
        &normalizer,
    );

    metrics.location_overlap = calculate_type_similarity(
        source_entities,
        target_entities,
        EntityType::Location,
        &normalizer,
    );

    metrics.event_overlap = calculate_type_similarity(
        source_entities,
        target_entities,
        EntityType::Event,
        &normalizer,
    );

    metrics.product_overlap = calculate_type_similarity(
        source_entities,
        target_entities,
        EntityType::Product,
        &normalizer,
    );

    // Log individual entity comparisons for debugging
    for source_entity in &source_entities.entities {
        for target_entity in &target_entities.entities {
            if source_entity.entity_type == target_entity.entity_type {
                let matches = normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type,
                );

                info!(
                    target: TARGET_ENTITY,
                    "Entity comparison: source={}({:?}), target={}({:?}), match={}",
                    source_entity.name, source_entity.importance,
                    target_entity.name, target_entity.importance,
                    matches
                );
            }
        }
    }

    // 3. Calculate primary entity overlap count
    metrics.primary_overlap_count =
        count_primary_overlap(source_entities, target_entities, &normalizer);

    // 4. Calculate temporal proximity (if dates available)
    metrics.temporal_proximity = calculate_temporal_proximity(
        source_entities.event_date.as_deref().or(source_date),
        target_entities.event_date.as_deref().or(target_date),
    );

    // 5. Calculate combined score from individual metrics
    metrics.calculate_combined_score();

    debug!(
        target: TARGET_ENTITY,
        "Entity similarity metrics: overlap_count={}, primary_overlap={}, person={:.2}, org={:.2}, location={:.2}, event={:.2}, product={:.2}, temporal={:.2}, combined={:.2}",
        metrics.entity_overlap_count,
        metrics.primary_overlap_count,
        metrics.person_overlap,
        metrics.organization_overlap,
        metrics.location_overlap,
        metrics.event_overlap,
        metrics.product_overlap,
        metrics.temporal_proximity,
        metrics.combined_score
    );

    // Critical error if we have overlapping entities but zero score
    if metrics.entity_overlap_count > 0 && metrics.combined_score == 0.0 {
        error!(
            target: TARGET_ENTITY,
            "CRITICAL ERROR: Entity similarity calculation produced zero score despite {} overlapping entities",
            metrics.entity_overlap_count
        );
    }

    metrics
}

/// Count how many entities overlap between two articles
fn count_entity_overlap(
    source_entities: &ExtractedEntities,
    target_entities: &ExtractedEntities,
    normalizer: &EntityNormalizer,
) -> usize {
    let mut overlap_count = 0;

    // For each source entity, check if there's a matching target entity
    for source_entity in &source_entities.entities {
        for target_entity in &target_entities.entities {
            // Entities must be of the same type
            if source_entity.entity_type == target_entity.entity_type {
                // Check if names match using the normalizer
                if normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type,
                ) {
                    overlap_count += 1;
                    break; // Count each source entity only once
                }
            }
        }
    }

    overlap_count
}

/// Calculate similarity score for a specific entity type
fn calculate_type_similarity(
    source_entities: &ExtractedEntities,
    target_entities: &ExtractedEntities,
    entity_type: EntityType,
    normalizer: &EntityNormalizer,
) -> f32 {
    // Get entities of the specified type
    let source_type_entities = source_entities.get_entities_by_type(entity_type);
    let target_type_entities = target_entities.get_entities_by_type(entity_type);

    // Empty sets edge case
    if source_type_entities.is_empty() || target_type_entities.is_empty() {
        return 0.0;
    }

    // Weights by importance
    const PRIMARY_WEIGHT: f32 = 1.0;
    const SECONDARY_WEIGHT: f32 = 0.6;
    const MENTIONED_WEIGHT: f32 = 0.3;

    // Calculate weighted overlap score
    let mut overlap_score = 0.0;
    let mut matched_target_indices = HashSet::new();

    // For each source entity, find the best matching target entity
    for source_entity in &source_type_entities {
        let source_weight = match source_entity.importance {
            ImportanceLevel::Primary => PRIMARY_WEIGHT,
            ImportanceLevel::Secondary => SECONDARY_WEIGHT,
            ImportanceLevel::Mentioned => MENTIONED_WEIGHT,
        };

        // Find best matching target entity that hasn't been matched yet
        for (target_idx, target_entity) in target_type_entities.iter().enumerate() {
            // Skip already matched target entities
            if matched_target_indices.contains(&target_idx) {
                continue;
            }

            // Check if entities match
            if normalizer.names_match(
                &source_entity.normalized_name,
                &target_entity.normalized_name,
                entity_type,
            ) {
                let target_weight = match target_entity.importance {
                    ImportanceLevel::Primary => PRIMARY_WEIGHT,
                    ImportanceLevel::Secondary => SECONDARY_WEIGHT,
                    ImportanceLevel::Mentioned => MENTIONED_WEIGHT,
                };

                // Average the weights for entities that match
                let combined_weight = (source_weight + target_weight) / 2.0;
                overlap_score += combined_weight;

                // Mark this target entity as matched
                matched_target_indices.insert(target_idx);
                break;
            }
        }
    }

    // Normalize score - divide by the theoretical maximum score if all entities matched
    let max_possible_score =
        (source_type_entities.len() + target_type_entities.len()) as f32 / 2.0 * PRIMARY_WEIGHT;
    if max_possible_score > 0.0 {
        overlap_score / max_possible_score
    } else {
        0.0
    }
}

/// Count primary entities that overlap between articles
fn count_primary_overlap(
    source_entities: &ExtractedEntities,
    target_entities: &ExtractedEntities,
    normalizer: &EntityNormalizer,
) -> usize {
    // Get primary entities
    let source_primary = source_entities.get_primary_entities();
    let target_primary = target_entities.get_primary_entities();

    let mut overlap_count = 0;

    // For each source primary entity, check if there's a matching target primary entity
    for source_entity in &source_primary {
        for target_entity in &target_primary {
            // Entities must be of the same type
            if source_entity.entity_type == target_entity.entity_type {
                // Check if names match using the normalizer
                if normalizer.names_match(
                    &source_entity.normalized_name,
                    &target_entity.normalized_name,
                    source_entity.entity_type,
                ) {
                    overlap_count += 1;
                    break; // Count each source entity only once
                }
            }
        }
    }

    overlap_count
}

/// Calculate temporal proximity score between article dates
fn calculate_temporal_proximity(source_date: Option<&str>, target_date: Option<&str>) -> f32 {
    match (source_date, target_date) {
        (Some(source), Some(target)) => {
            // Log the date strings we're comparing
            debug!(target: TARGET_ENTITY, "Comparing dates: source={}, target={}", source, target);

            // Try to parse dates (multiple formats)
            let source_parsed = parse_date(source);
            let target_parsed = parse_date(target);

            match (source_parsed, target_parsed) {
                (Some(s_date), Some(t_date)) => {
                    debug!(target: TARGET_ENTITY, "Successfully parsed dates: source={}, target={}", s_date, t_date);
                    // Calculate days between dates
                    let days_diff = (s_date - t_date).num_days().abs();

                    // Convert to proximity score (1.0 = same day, decreasing with difference)
                    // We use a sigmoid-like decay function:
                    match days_diff {
                        0 => 1.0,        // Same day
                        1 => 0.9,        // Adjacent days
                        2..=7 => 0.7,    // Same week
                        8..=30 => 0.5,   // Same month
                        31..=90 => 0.3,  // Same quarter
                        91..=365 => 0.1, // Same year
                        _ => 0.0,        // More than a year apart
                    }
                }
                _ => {
                    warn!(target: TARGET_ENTITY, "Failed to parse dates for temporal proximity: {} and {}", source, target);
                    0.0 // Couldn't compare dates
                }
            }
        }
        _ => 0.0, // Missing date information
    }
}

/// Parse date string into NaiveDate, trying multiple formats
fn parse_date(date_str: &str) -> Option<NaiveDate> {
    // Try RFC3339 format with timezone (2025-04-29T05:17:23+00:00)
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(date_str) {
        return Some(datetime.date_naive());
    }

    // Try ISO format with time but no timezone (2025-04-29T05:17:23)
    if let Ok(datetime) = chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
        return Some(datetime.date());
    }

    // Try ISO format (YYYY-MM-DD)
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Some(date);
    }

    // Try common formats
    let formats = [
        "%Y/%m/%d",  // 2025/01/15
        "%d/%m/%Y",  // 15/01/2025
        "%m/%d/%Y",  // 01/15/2025
        "%B %d, %Y", // January 15, 2025
        "%d %B %Y",  // 15 January 2025
        "%Y-%m",     // 2025-01 (month precision)
        "%Y",        // 2025 (year precision)
    ];

    for format in &formats {
        if let Ok(date) = NaiveDate::parse_from_str(date_str, format) {
            return Some(date);
        }
    }

    // Handle quarter format (2025-Q1)
    if let Some(captures) = regex::Regex::new(r"(\d{4})-Q([1-4])")
        .ok()
        .and_then(|re| re.captures(date_str))
    {
        if let (Some(year_str), Some(quarter_str)) = (captures.get(1), captures.get(2)) {
            if let (Ok(year), Ok(quarter)) = (
                year_str.as_str().parse::<i32>(),
                quarter_str.as_str().parse::<u32>(),
            ) {
                // Convert quarter to month (Q1->1, Q2->4, Q3->7, Q4->10)
                let month = (quarter - 1) * 3 + 1;
                return NaiveDate::from_ymd_opt(year, month, 1);
            }
        }
    }

    // Try to extract year if all else fails
    if let Some(captures) = regex::Regex::new(r"(\d{4})")
        .ok()
        .and_then(|re| re.captures(date_str))
    {
        if let Some(year_str) = captures.get(1) {
            if let Ok(year) = year_str.as_str().parse::<i32>() {
                return NaiveDate::from_ymd_opt(year, 1, 1);
            }
        }
    }

    None
}
