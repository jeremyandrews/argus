use anyhow::Result;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{SearchParams, SearchPoints, WithPayloadSelector, WithVectorsSelector};
use qdrant_client::Qdrant;
use sqlx;
use tracing::{error, info, warn};

use crate::entity;
use crate::vector::{
    similarity::{calculate_direct_similarity, calculate_similarity_date_threshold},
    storage::get_article_vector_from_qdrant,
    types::{ArticleMatch, EnhancedArticleMatch, NearMissMatch},
    QDRANT_URL_ENV, TARGET_VECTOR,
};

/// Search for similar articles using vector similarity
pub async fn get_similar_articles(embedding: &Vec<f32>, limit: u64) -> Result<Vec<ArticleMatch>> {
    info!(target: TARGET_VECTOR, 
        "search_similar_articles: embedding length = {}, using similarity time window of {} days", 
        embedding.len(), crate::vector::similarity::SIMILARITY_TIME_WINDOW_DAYS);

    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    // Calculate date threshold for recent articles
    let date_threshold = calculate_similarity_date_threshold();
    info!(target: TARGET_VECTOR, "Using date threshold for similarity search: {}", date_threshold);

    // Parse the threshold date
    let parsed_date = chrono::DateTime::parse_from_rfc3339(&date_threshold).unwrap();
    let timestamp_seconds = parsed_date.timestamp();

    // Create the search request with date filter
    let search_points = SearchPoints {
        collection_name: "articles".to_string(),
        vector: embedding.clone(),
        limit,
        with_payload: Some(WithPayloadSelector::from(true)),
        with_vectors: Some(WithVectorsSelector::from(false)),
        filter: Some(qdrant_client::qdrant::Filter {
            must: vec![qdrant_client::qdrant::Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    qdrant_client::qdrant::FieldCondition {
                        key: "published_date".to_string(),
                        r#match: None,
                        range: None,
                        datetime_range: Some(qdrant_client::qdrant::DatetimeRange {
                            gt: Some(qdrant_client::qdrant::Timestamp {
                                seconds: timestamp_seconds,
                                nanos: 0,
                            }),
                            lt: None,
                            gte: None,
                            lte: None,
                        }),
                        geo_bounding_box: None,
                        geo_radius: None,
                        values_count: None,
                        geo_polygon: None,
                    },
                )),
            }],
            should: vec![],
            must_not: vec![],
            min_should: None,
        }),
        params: Some(SearchParams {
            hnsw_ef: Some(128),
            exact: Some(true),
            ..Default::default()
        }),
        score_threshold: Some(0.80),
        // The sort field doesn't exist on SearchPoints, we'll sort after fetching
        ..Default::default()
    };

    match client.search_points(search_points).await {
        Ok(response) => {
            let mut matches: Vec<ArticleMatch> = response
                .result
                .into_iter()
                .map(|scored_point| {
                    let id = match scored_point.id.unwrap().point_id_options.unwrap() {
                        PointIdOptions::Num(num) => num as i64,
                        _ => panic!("Expected numeric point ID"),
                    };

                    let payload = scored_point.payload;
                    let published_date = payload
                        .get("published_date")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::StringValue(s) = k {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();

                    let category = payload
                        .get("category")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::StringValue(s) = k {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();

                    let quality_score = payload
                        .get("quality_score")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::IntegerValue(i) = k {
                                Some(*i as i8)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);

                    ArticleMatch {
                        id,
                        published_date,
                        category,
                        quality_score,
                        score: scored_point.score,

                        // New fields with default values
                        vector_score: Some(scored_point.score), // Vector score is the same as score for pure vector matches
                        vector_active_dimensions: None,
                        vector_magnitude: None,
                        entity_overlap_count: None,
                        primary_overlap_count: None,
                        person_overlap: None,
                        org_overlap: None,
                        location_overlap: None,
                        event_overlap: None,
                        temporal_proximity: None,
                        similarity_formula: Some(
                            "60% vector similarity (no entity data available)".to_string(),
                        ),
                    }
                })
                .collect();

            // Sort by quality_score in descending order
            matches.sort_by(|a, b| b.quality_score.cmp(&a.quality_score));

            info!(
                target: TARGET_VECTOR,
                "Found {} similar articles", matches.len()
            );
            Ok(matches)
        }
        Err(e) => {
            error!(
                target: TARGET_VECTOR,
                "Failed to search for similar articles: {:?}", e
            );
            Err(anyhow::anyhow!(
                "Failed to search for similar articles: {:?}",
                e
            ))
        }
    }
}

/// Get all entities for a specific article
pub async fn get_article_entities(article_id: i64) -> Result<Option<entity::ExtractedEntities>> {
    let db = crate::db::core::Database::instance().await;

    // Get article's date information
    let (_pub_date, event_date) = db.get_article_details_with_dates(article_id).await?;

    // Create an extracted entities object
    let mut extracted = entity::ExtractedEntities::new();

    // Set event date if available
    if let Some(date) = event_date {
        extracted = extracted.with_event_date(&date);
    }

    // Get all entities linked to this article
    let entities = db.get_article_entities(article_id).await?;

    // Convert database rows to Entity objects
    for (entity_id, name, entity_type_str, importance_str) in entities {
        let entity_type = entity::EntityType::from(entity_type_str.as_str());
        let importance = entity::ImportanceLevel::from(importance_str.as_str());

        // Create entity and add to collection
        let entity = entity::Entity::new(&name, &name.to_lowercase(), entity_type, importance)
            .with_id(entity_id);

        extracted.add_entity(entity);
    }

    if extracted.entities.is_empty() {
        return Ok(None);
    }

    Ok(Some(extracted))
}

/// Get articles that share significant entities with the given entity IDs
async fn get_articles_by_entities(
    entity_ids: &[i64],
    limit: u64,
    source_article_id: Option<i64>,
) -> Result<Vec<ArticleMatch>> {
    let db = crate::db::core::Database::instance().await;

    // Get the source article's publication date if we have a source article ID
    let source_date = if let Some(id) = source_article_id {
        match db.get_article_details_with_dates(id).await {
            Ok((pub_date, _)) => pub_date,
            Err(e) => {
                error!(target: TARGET_VECTOR, "Failed to get source article date: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Use the article's own date, or current date as fallback
    let date_for_search = source_date.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    info!(target: TARGET_VECTOR, "Using source date for article similarity: {}", date_for_search);

    // Use the database function to get articles by entities within a date window
    let entity_matches = db
        .get_articles_by_entities_with_date(entity_ids, limit, &date_for_search)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get articles by entities: {}", e))?;

    // Convert to ArticleMatch objects
    let matches: Vec<ArticleMatch> = entity_matches
        .into_iter()
        .map(
            |(id, published_date, category, primary_count, total_count)| {
                // Use default quality score of 0
                let quality_score = 0i8;

                // Calculate a preliminary score based on entity overlap
                let primary_count = primary_count as f32;
                let total_count = total_count as f32;

                // Score formula: prioritize PRIMARY entities but also consider total overlap
                let entity_score = if total_count > 0.0 {
                    (0.7 * (primary_count / entity_ids.len() as f32))
                        + (0.3 * (total_count / entity_ids.len() as f32))
                } else {
                    0.0
                };

                ArticleMatch {
                    id,
                    published_date: published_date.unwrap_or_default(),
                    category: category.unwrap_or_default(),
                    quality_score,
                    score: entity_score, // Preliminary score, will be refined later

                    // Add entity metrics fields
                    vector_score: None, // Will be calculated later
                    vector_active_dimensions: None,
                    vector_magnitude: None,
                    entity_overlap_count: Some(total_count as usize),
                    primary_overlap_count: Some(primary_count as usize),
                    person_overlap: None, // Detailed metrics not available at this stage
                    org_overlap: None,
                    location_overlap: None,
                    event_overlap: None,
                    temporal_proximity: None,
                    similarity_formula: Some(format!("Entity-based score: 70% primary entities ({} of {}) + 30% total entities ({} of {})",
                        primary_count as usize, entity_ids.len(), total_count as usize, entity_ids.len())),
                }
            },
        )
        .collect();

    Ok(matches)
}

/// Build an ExtractedEntities object from entity IDs
async fn build_entities_from_ids(entity_ids: &[i64]) -> Result<entity::ExtractedEntities> {
    let db = crate::db::core::Database::instance().await;
    let mut extracted = entity::ExtractedEntities::new();

    info!(target: TARGET_VECTOR, "Building source entities from {} entity IDs with detailed tracing: {:?}", 
          entity_ids.len(), entity_ids);

    // If there are entity IDs, try to determine the article ID they're from
    // by checking the article_entities table
    if !entity_ids.is_empty() {
        let article_id_result = sqlx::query_scalar::<_, i64>(
            "SELECT article_id FROM article_entities WHERE entity_id = ? LIMIT 1",
        )
        .bind(entity_ids[0]) // Just use the first entity ID to find the article
        .fetch_optional(db.pool())
        .await;

        if let Ok(Some(article_id)) = article_id_result {
            // Found source article - get all entities with proper relationships
            info!(target: TARGET_VECTOR, "Found source article ID {} for entity ID {}", 
                  article_id, entity_ids[0]);

            match db.get_article_entities(article_id).await {
                Ok(article_entities) => {
                    info!(target: TARGET_VECTOR, "Database returned {} total entities for article {}", 
                          article_entities.len(), article_id);

                    // Filter to include only entities in our entity_ids list
                    for (entity_id, name, entity_type_str, importance_str) in article_entities {
                        if entity_ids.contains(&entity_id) {
                            let entity_type = entity::EntityType::from(entity_type_str.as_str());
                            let importance = entity::ImportanceLevel::from(importance_str.as_str());

                            let entity = entity::Entity::new(
                                &name,
                                &name.to_lowercase(),
                                entity_type,
                                importance, // Use actual importance from database
                            )
                            .with_id(entity_id);

                            info!(target: TARGET_VECTOR, 
                                "Added entity with proper relationship: id={}, name='{}', type={}, importance={}", 
                                entity_id, name, entity_type_str, importance_str);
                            extracted.add_entity(entity);
                        }
                    }

                    // If we didn't match all entities, fall back to basic lookup for the rest
                    if extracted.entities.len() < entity_ids.len() {
                        let found_ids: std::collections::HashSet<i64> =
                            extracted.entities.iter().filter_map(|e| e.id).collect();

                        let missing_ids: Vec<i64> = entity_ids
                            .iter()
                            .filter(|&&id| !found_ids.contains(&id))
                            .copied()
                            .collect();

                        info!(target: TARGET_VECTOR, 
                              "Missing {} entities, falling back to basic lookup for IDs: {:?}", 
                              missing_ids.len(), missing_ids);

                        // Fall back to basic lookup for missing entities
                        for &id in &missing_ids {
                            if let Ok(Some((name, entity_type_str, _parent_id))) =
                                db.get_entity_details(id).await
                            {
                                let entity_type =
                                    entity::EntityType::from(entity_type_str.as_str());

                                let entity = entity::Entity::new(
                                    &name,
                                    &name.to_lowercase(),
                                    entity_type,
                                    entity::ImportanceLevel::Primary, // Default to PRIMARY for fallback
                                )
                                .with_id(id);

                                info!(target: TARGET_VECTOR, 
                                      "Added fallback entity: id={}, name='{}', type={}", 
                                      id, name, entity_type_str);
                                extracted.add_entity(entity);
                            } else {
                                error!(target: TARGET_VECTOR, 
                                      "Failed to get details for entity ID {} - entity is missing from database", id);
                            }
                        }
                    }

                    // Add detailed entity breakdown
                    info!(
                        target: TARGET_VECTOR,
                        "Entity retrieval details: total entities={}, by importance: PRIMARY={}, SECONDARY={}, MENTIONED={}",
                        extracted.entities.len(),
                        extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Primary).count(),
                        extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Secondary).count(),
                        extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Mentioned).count()
                    );

                    // Add entity type breakdown
                    info!(
                        target: TARGET_VECTOR,
                        "Entity types breakdown: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
                        extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Person).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Organization).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Location).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Event).count()
                    );

                    info!(target: TARGET_VECTOR, "Built {} entities using article relationship data", 
                          extracted.entities.len());
                    return Ok(extracted);
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, 
                          "Failed to get article entities for article {}: {}", article_id, e);
                    // Fall through to basic lookup below
                }
            }
        } else if let Err(e) = article_id_result {
            error!(target: TARGET_VECTOR, 
                  "Database error when trying to find source article for entity ID {}: {}", entity_ids[0], e);
        } else {
            warn!(target: TARGET_VECTOR, 
                 "Could not determine source article ID for entity ID {}", entity_ids[0]);
        }
    }

    // Fall back to basic entity lookup if we couldn't find relationship data
    for &id in entity_ids {
        if let Ok(Some((name, entity_type_str, _parent_id))) = db.get_entity_details(id).await {
            let entity_type = entity::EntityType::from(entity_type_str.as_str());

            // Create entity with PRIMARY importance (these are our source entities)
            let entity = entity::Entity::new(
                &name,
                &name.to_lowercase(),
                entity_type,
                entity::ImportanceLevel::Primary,
            )
            .with_id(id);

            info!(target: TARGET_VECTOR, "Added source entity: id={}, name='{}', type={}", 
                  id, name, entity_type_str);
            extracted.add_entity(entity);
        } else {
            error!(target: TARGET_VECTOR, 
                  "Failed to get details for entity ID {} - entity is missing from database", id);
        }
    }

    // Critical error if we have no entities despite having IDs
    if extracted.entities.is_empty() && !entity_ids.is_empty() {
        error!(
            target: TARGET_VECTOR,
            "CRITICAL: Failed to retrieve any entities despite having {} entity IDs: {:?}",
            entity_ids.len(), entity_ids
        );
    }

    // Add detailed entity breakdown
    if !extracted.entities.is_empty() {
        info!(
            target: TARGET_VECTOR,
            "Entity retrieval details: total entities={}, by importance: PRIMARY={}, SECONDARY={}, MENTIONED={}",
            extracted.entities.len(),
            extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Primary).count(),
            extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Secondary).count(),
            extracted.entities.iter().filter(|e| e.importance == entity::ImportanceLevel::Mentioned).count()
        );

        // Add entity type breakdown
        info!(
            target: TARGET_VECTOR,
            "Entity types breakdown: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
            extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Person).count(),
            extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Organization).count(),
            extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Location).count(),
            extracted.entities.iter().filter(|e| e.entity_type == entity::EntityType::Event).count()
        );
    }

    info!(target: TARGET_VECTOR, "Built {} source entities from {} entity IDs using basic lookup", 
          extracted.entities.len(), entity_ids.len());

    Ok(extracted)
}

/// Find similar articles using a dual-query approach that combines vector similarity with entity matching
pub async fn get_similar_articles_with_entities(
    embedding: &Vec<f32>,
    limit: u64,
    entity_ids: Option<&[i64]>,
    event_date: Option<&str>,
    source_article_id: Option<i64>, // For tracking the source article
) -> Result<Vec<ArticleMatch>> {
    if let Some(id) = source_article_id {
        info!(target: TARGET_VECTOR, "Starting similarity search for source article ID: {}", id);
    }

    info!(target: TARGET_VECTOR, "Starting enhanced entity-aware article search with dual-query approach");

    // Log entity IDs detail
    if let Some(ids) = entity_ids {
        info!(target: TARGET_VECTOR, "Using {} entity IDs for similar article search: {:?}", 
              ids.len(), ids);

        // Check if these entities exist in the database
        if let Some(id) = source_article_id {
            let db = crate::db::core::Database::instance().await;
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM article_entities WHERE article_id = ?",
            )
            .bind(id)
            .fetch_one(db.pool())
            .await
            {
                Ok(count) => {
                    info!(target: TARGET_VECTOR, "Article {} has {} entities in the database", id, count);
                    if count == 0 && ids.len() > 0 {
                        warn!(target: TARGET_VECTOR, 
                             "Database shows 0 entities for article {} but received {} entity IDs", 
                             id, ids.len());
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "Failed to check entity count for article {}: {}", id, e);
                }
            }
        }
    } else {
        info!(target: TARGET_VECTOR, "No entity IDs provided for similar article search");
    }

    let mut all_matches = std::collections::HashMap::new();

    // 1. Vector-based query using current implementation
    info!(target: TARGET_VECTOR, "Performing vector similarity search...");
    let vector_matches = get_similar_articles(embedding, limit * 2).await?; // Get more results for better coverage

    // Store the count before consuming the vector
    let vector_only_count = vector_matches.len();
    info!(target: TARGET_VECTOR, "Vector search returned {} results", vector_only_count);

    // Add vector matches to our result set
    for article in vector_matches {
        all_matches.insert(article.id, article);
    }

    // 2. Entity-based query (if we have entity IDs)
    if let Some(ids) = entity_ids {
        if !ids.is_empty() {
            info!(target: TARGET_VECTOR, "Performing entity-based search with {} entity IDs...", ids.len());
            // SET LOG LEVEL TO TRACE/DEBUG
            info!(target: TARGET_VECTOR, 
                "CRITICAL DEBUG: About to call get_articles_by_entities with source_article_id: {:?}", 
                source_article_id);

            match get_articles_by_entities(ids, limit * 2, source_article_id).await {
                Ok(entity_matches) => {
                    info!(target: TARGET_VECTOR, 
                        "Entity search returned {} results for entity IDs: {:?}",
                        entity_matches.len(), ids);

                    if entity_matches.is_empty() {
                        error!(target: TARGET_VECTOR, 
                            "CRITICAL: Entity search returned NO matches despite having valid entity IDs - database inconsistency possible");
                    }

                    // Continue processing with entity_matches
                    // For entity matches, calculate vector similarity if not already included
                    for mut article in entity_matches {
                        if !all_matches.contains_key(&article.id) {
                            // Special handling for self-comparisons (when article is comparing to itself)
                            if source_article_id.is_some()
                                && article.id == source_article_id.unwrap()
                            {
                                info!(target: TARGET_VECTOR, 
                                    "Skipping vector calculation for self-comparison of article {}", article.id);

                                // When comparing to self, the vector similarity is 1.0 (identical)
                                article.vector_score = Some(1.0);
                                article.score = 1.0;
                                all_matches.insert(article.id, article);
                                continue;
                            }

                            // For other articles, calculate vector similarity directly
                            match get_article_vector_from_qdrant(article.id).await {
                                Ok(target_vector) => {
                                    match calculate_direct_similarity(embedding, &target_vector) {
                                        Ok(vector_score) => {
                                            info!(target: TARGET_VECTOR, 
                                                "Added entity-based match: article_id={}, entity_overlap={}, vector_score={:.4}",
                                                article.id, article.entity_overlap_count.unwrap_or(0), vector_score);
                                            article.vector_score = Some(vector_score);
                                            article.score = vector_score; // Update with actual vector score
                                            all_matches.insert(article.id, article);
                                        }
                                        Err(e) => {
                                            error!(target: TARGET_VECTOR, 
                                                "Failed to calculate direct similarity for article {}: {:?}", 
                                                article.id, e);
                                            // Still include the article even if we couldn't get vector similarity
                                            article.score = 0.0;
                                            all_matches.insert(article.id, article);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(target: TARGET_VECTOR, 
                                        "Failed to retrieve vector for article {}: {:?}", article.id, e);
                                    // Still include the article even if we couldn't get the vector
                                    article.score = 0.0;
                                    all_matches.insert(article.id, article);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "CRITICAL ERROR in get_articles_by_entities: {:?}", e);
                }
            }
        }
    }

    if all_matches.is_empty() {
        error!(target: TARGET_VECTOR, 
            "CRITICAL: No matches found through either vector or entity-based search");
        return Ok(vec![]);
    }

    info!(target: TARGET_VECTOR, "Found {} total unique articles from both queries", all_matches.len());

    // Calculate entity-only count
    let entity_only_count = entity_ids.map_or(0, |ids| {
        if ids.is_empty() {
            0
        } else {
            all_matches.len() - vector_only_count
        }
    });

    info!(target: TARGET_VECTOR, 
        "Match sources: {} from vector similarity, {} from entity similarity",
        vector_only_count, entity_only_count);

    // 3. Now enhance all matches with entity similarity scores
    let mut enhanced_matches = Vec::new();
    for (id, article) in all_matches {
        // Get entity data for this article
        let article_entities = match get_article_entities(id).await {
            Ok(Some(entities)) => entities,
            Ok(None) => {
                // Still include articles without entities
                let enhanced = EnhancedArticleMatch {
                    article_id: id,
                    vector_score: article.score,
                    entity_similarity: entity::EntitySimilarityMetrics::new(),
                    final_score: 0.6 * article.score, // Apply consistent 60% weighting
                    category: article.category,
                    published_date: article.published_date,
                    quality_score: article.quality_score,
                };
                enhanced_matches.push(enhanced);
                continue;
            }
            Err(e) => {
                error!(target: TARGET_VECTOR, "Failed to get entities for article {}: {:?}", id, e);
                continue;
            }
        };

        // If we have both our source entities and this article's entities, calculate similarity
        if let Some(ids) = entity_ids {
            // Create a source entities object from the IDs, passing the source article ID
            let source_entities = match build_entities_from_ids(ids).await {
                Ok(entities) => {
                    if entities.entities.is_empty() && source_article_id.is_some() {
                        // If we got no entities but have a source article ID, try to get them directly
                        match get_article_entities(source_article_id.unwrap()).await {
                            Ok(Some(direct_entities)) => {
                                info!(target: TARGET_VECTOR, 
                                    "Retrieved entities directly from source article {}", 
                                    source_article_id.unwrap());
                                direct_entities
                            }
                            _ => entities,
                        }
                    } else {
                        entities
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "Failed to build source entities: {:?}", e);
                    // Try direct retrieval as fallback if we have source article ID
                    if let Some(id) = source_article_id {
                        match get_article_entities(id).await {
                            Ok(Some(entities)) => {
                                info!(target: TARGET_VECTOR, 
                                    "Retrieved entities directly after build failure for article {}", id);
                                entities
                            }
                            _ => entity::ExtractedEntities::new(), // Empty fallback
                        }
                    } else {
                        entity::ExtractedEntities::new() // Empty fallback
                    }
                }
            };

            // Calculate entity similarity between articles
            let entity_sim = entity::matching::calculate_entity_similarity(
                &source_entities,
                &article_entities,
                event_date,
                Some(&article.published_date),
            );

            // Log entity similarity calculation details
            info!(target: TARGET_VECTOR,
                "Entity similarity for article {}: entity_score={:.4}, person={:.2}, org={:.2}, location={:.2}, event={:.2}, overlap_count={}",
                id, entity_sim.combined_score,
                entity_sim.person_overlap, entity_sim.organization_overlap,
                entity_sim.location_overlap, entity_sim.event_overlap,
                entity_sim.entity_overlap_count
            );

            // Create enhanced match with combined score
            let enhanced = EnhancedArticleMatch {
                article_id: id,
                vector_score: article.score,
                entity_similarity: entity_sim.clone(),
                // Combined score: 60% vector + 40% entity (as specified in activeContext.md)
                final_score: 0.6 * article.score + 0.4 * entity_sim.combined_score,
                category: article.category,
                published_date: article.published_date,
                quality_score: article.quality_score,
            };

            enhanced_matches.push(enhanced);
        } else {
            // Without source entities, apply consistent 60% weighting to vector score
            // Create a new entity metrics with defaults
            let empty_metrics = entity::EntitySimilarityMetrics::new();

            let enhanced = EnhancedArticleMatch {
                article_id: id,
                vector_score: article.score,
                entity_similarity: empty_metrics,
                final_score: 0.6 * article.score, // Apply consistent 60% weighting
                category: article.category,
                published_date: article.published_date,
                quality_score: article.quality_score,
            };
            enhanced_matches.push(enhanced);
        }
    }

    // 4. Sort by final_score and apply threshold
    enhanced_matches.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    info!(target: TARGET_VECTOR, "Enhanced matches before filtering: {}", enhanced_matches.len());

    // Log all candidate matches before filtering
    for m in &enhanced_matches {
        info!(target: TARGET_VECTOR,
            "PRE-FILTER: article_id={}, vector_score={:.4}, entity_score={:.4}, final_score={:.4}, entity_overlap={}, primary_overlap={}",
            m.article_id, m.vector_score, m.entity_similarity.combined_score, m.final_score,
            m.entity_similarity.entity_overlap_count, m.entity_similarity.primary_overlap_count
        );
    }

    // Track near-miss matches for diagnostic purposes
    let similarity_threshold = 0.70;
    let mut near_misses = Vec::new();

    // Apply minimum combined threshold (0.75) and filter out self-matches
    let final_matches: Vec<ArticleMatch> = enhanced_matches
                .into_iter()
                .filter(|m| {
                    // First, filter out self-matches (articles shouldn't match themselves)
                    if source_article_id.is_some() && m.article_id == source_article_id.unwrap() {
                        info!(target: TARGET_VECTOR, "Filtered out self-match for article {}", m.article_id);
                        return false;
                    }

                    // Then check similarity threshold
                    let passes = m.final_score >= similarity_threshold;
                    if !passes {
                        // Create a near-miss record for this article
                        let missing_score = similarity_threshold - m.final_score;
                        let reason = if m.entity_similarity.entity_overlap_count == 0 {
                            "No entity overlap".to_string()
                        } else if m.entity_similarity.combined_score < 0.3 {
                            format!("Weak entity similarity ({:.2})", m.entity_similarity.combined_score)
                        } else if m.vector_score < 0.5 {
                            format!("Low vector similarity ({:.2})", m.vector_score)
                        } else {
                            "Combined score below threshold".to_string()
                        };

                        // Log details about why it didn't match
                        info!(target: TARGET_VECTOR,
                            "NEAR MISS: article_id={}, final_score={:.4} (below {:.2}), vector={:.4}, entity={:.4}, overlap={}, reason={}",
                            m.article_id, m.final_score, similarity_threshold, m.vector_score,
                            m.entity_similarity.combined_score, m.entity_similarity.entity_overlap_count, reason
                        );

                        // Add to near-miss collection
                        near_misses.push(NearMissMatch {
                            article_id: m.article_id,
                            score: m.final_score,
                            threshold: similarity_threshold,
                            missing_score: missing_score,
                            vector_score: Some(m.vector_score),
                            entity_score: Some(m.entity_similarity.combined_score),
                            entity_overlap_count: Some(m.entity_similarity.entity_overlap_count),
                            reason,
                        });
                    }
                    passes
                })
        .take(limit as usize)
        .map(|m| {
            info!(target: TARGET_VECTOR,
                "Match article_id={}, vector_score={:.4}, entity_score={:.4}, final_score={:.4}, primary_overlap={}",
                m.article_id, m.vector_score, m.entity_similarity.combined_score, m.final_score, m.entity_similarity.primary_overlap_count
            );

            // Create the formula string
            let formula = format!(
                "60% vector similarity ({:.2}) + 40% entity similarity ({:.2}), where entity similarity combines person (30%), organization (20%), location (15%), event (15%), and temporal (20%) factors",
                m.vector_score,
                m.entity_similarity.combined_score
            );

            ArticleMatch {
                id: m.article_id,
                published_date: m.published_date,
                category: m.category,
                quality_score: m.quality_score,
                score: m.final_score, // Use the combined score

                // Add vector metrics
                vector_score: Some(m.vector_score),
                vector_active_dimensions: None, // Not tracked for enhanced matches
                vector_magnitude: None, // Not tracked for enhanced matches

                // Add entity metrics
                entity_overlap_count: Some(m.entity_similarity.entity_overlap_count),
                primary_overlap_count: Some(m.entity_similarity.primary_overlap_count),
                person_overlap: Some(m.entity_similarity.person_overlap),
                org_overlap: Some(m.entity_similarity.organization_overlap),
                location_overlap: Some(m.entity_similarity.location_overlap),
                event_overlap: Some(m.entity_similarity.event_overlap),
                temporal_proximity: Some(m.entity_similarity.temporal_proximity),

                // Add formula explanation
                similarity_formula: Some(formula),
            }
        })
        .collect();

    info!(target: TARGET_VECTOR, "Final matched article count after filtering: {}", final_matches.len());

    // Add result verification logs to confirm all matches have entity overlap
    let with_entity_overlap = final_matches
        .iter()
        .filter(|m| m.entity_overlap_count.unwrap_or(0) > 0)
        .count();

    info!(target: TARGET_VECTOR,
        "FILTER RESULTS: Total matches: {}, With entity overlap: {}, No entity overlap: {}",
        final_matches.len(),
        with_entity_overlap,
        final_matches.len() - with_entity_overlap
    );

    if final_matches.len() - with_entity_overlap > 0 {
        // This should never happen with our fix, so log it as an error if it does
        error!(target: TARGET_VECTOR,
            "ERROR: Found {} articles without entity overlap that passed the filter threshold!",
            final_matches.len() - with_entity_overlap
        );
    }

    Ok(final_matches)
}
