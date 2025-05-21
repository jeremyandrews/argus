use crate::db::core::Database;
use crate::vector::{
    embedding::get_article_vectors,
    search::{get_similar_articles, get_similar_articles_with_entities},
    storage::store_embedding,
};
use crate::JsonSchemaType;
use serde_json::json;
use tokio::time::Instant;
use tracing::{debug, error, info};

/// Converts an ArticleMatch and article details into a standardized JSON representation
pub fn build_similar_article_json(
    article: &crate::vector::types::ArticleMatch,
    json_url: Option<String>,
    title: Option<String>,
    tiny_summary: Option<String>,
) -> serde_json::Value {
    json!({
        // Basic fields
        "id": article.id,
        "json_url": json_url.unwrap_or_else(|| "Unknown URL".to_string()),
        "title": title.unwrap_or_else(|| "Unknown Title".to_string()),
        "tiny_summary": tiny_summary.unwrap_or_default(),
        "category": article.category.clone(),
        "published_date": article.published_date.clone(),
        "quality_score": article.quality_score,
        "similarity_score": article.score,

        // Vector quality fields - Explicitly unwrap Option types with defaults
        "vector_score": article.vector_score.unwrap_or(0.0),
        "vector_active_dimensions": article.vector_active_dimensions.unwrap_or(0),
        "vector_magnitude": article.vector_magnitude.unwrap_or(0.0),

        // Entity similarity fields - Explicitly unwrap Option types with defaults
        "entity_overlap_count": article.entity_overlap_count.unwrap_or(0),
        "primary_overlap_count": article.primary_overlap_count.unwrap_or(0),
        "person_overlap": article.person_overlap.unwrap_or(0.0),
        "org_overlap": article.org_overlap.unwrap_or(0.0),
        "location_overlap": article.location_overlap.unwrap_or(0.0),
        "event_overlap": article.event_overlap.unwrap_or(0.0),
        "temporal_proximity": article.temporal_proximity.unwrap_or(0.0),

        // Formula explanation
        "similarity_formula": article.similarity_formula.as_ref().map_or_else(|| "Unknown".to_string(), |s| s.clone())
    })
}

/// Process vector embeddings, entity extraction, and similarity searches for an article
pub async fn process_article_similarity(
    db: &Database,
    article_id: i64,
    summary: &str,
    article_text: &str,
    pub_date: Option<&str>,
    _article_hash: &str,
    _title_domain_hash: &str,
    topic: Option<&str>,
    quality: i8,
    response_json: &mut serde_json::Value,
    llm_params: &mut crate::TextLLMParams,
    worker_detail: &crate::WorkerDetail,
) -> Result<(), anyhow::Error> {
    // Generate vector embedding
    let vector_start = Instant::now();
    if let Ok(Some(embedding)) = get_article_vectors(summary).await {
        info!(
            "Generated vector embedding with {} dimensions in {:?}",
            embedding.len(),
            vector_start.elapsed()
        );

        // FIRST: Extract entities BEFORE similarity search
        let entity_extraction_start = Instant::now();
        let mut entity_ids: Option<Vec<i64>> = None;

        // Create JsonLLMParams directly
        let json_params = crate::JsonLLMParams {
            base: llm_params.base.clone(),
            schema_type: JsonSchemaType::EntityExtraction,
        };

        match crate::entity::extraction::extract_entities(
            article_text,
            pub_date,
            &json_params,
            worker_detail,
        )
        .await
        {
            Ok(extracted_entities) => {
                info!(
                    "Extracted {} entities in {:?}",
                    extracted_entities.entities.len(),
                    entity_extraction_start.elapsed()
                );

                // Convert to JSON for database storage
                let entities_json =
                    serde_json::to_string(&extracted_entities).unwrap_or_else(|_| "{}".to_string());

                // Store entities and get the IDs
                match db
                    .process_entity_extraction(article_id, &entities_json)
                    .await
                {
                    Ok(ids) => {
                        info!(
                            "Successfully processed entity extraction for article {} with {} entities",
                            article_id, ids.len()
                        );
                        entity_ids = Some(ids);

                        // Extract potential aliases from article text
                        let alias_extraction_start = Instant::now();
                        let potential_aliases = crate::entity::aliases::extract_potential_aliases(
                            article_text,
                            None, // Let the function infer entity types
                        );

                        info!(
                            "Extracted {} potential aliases in {:?}",
                            potential_aliases.len(),
                            alias_extraction_start.elapsed()
                        );

                        // Store each potential alias in the database
                        for (canonical, alias, entity_type, confidence) in potential_aliases {
                            match crate::entity::aliases::add_alias(
                                db,
                                None, // No entity_id until approved
                                &canonical,
                                &alias,
                                entity_type,
                                "pattern", // Source is pattern-based extraction
                                confidence,
                            )
                            .await
                            {
                                Ok(alias_id) => {
                                    if alias_id > 0 {
                                        debug!(
                                            "Added potential alias: '{}' ↔ '{}' ({:?}) with confidence {:.2}",
                                            canonical, alias, entity_type, confidence
                                        );
                                    }
                                }
                                Err(e) => {
                                    debug!(
                                        "Failed to add potential alias: {} ↔ {} - {:?}",
                                        canonical, alias, e
                                    );
                                }
                            }
                        }

                        // Assign article to a cluster
                        let cluster_start = Instant::now();
                        match crate::clustering::assign_article_to_cluster(db, article_id).await {
                            Ok(cluster_id) => {
                                if cluster_id > 0 {
                                    info!(
                                        "Assigned article {} to cluster {} in {:?}",
                                        article_id,
                                        cluster_id,
                                        cluster_start.elapsed()
                                    );

                                    // Generate summary for the cluster
                                    match crate::clustering::generate_cluster_summary(
                                        db,
                                        &llm_params.base.llm_client,
                                        cluster_id,
                                    )
                                    .await
                                    {
                                        Ok(summary) => {
                                            info!(
                                                "Generated summary for cluster {} (length: {})",
                                                cluster_id,
                                                summary.len()
                                            );

                                            // Update cluster significance
                                            if let Ok(score) =
                                                crate::clustering::calculate_cluster_significance(
                                                    db, cluster_id,
                                                )
                                                .await
                                            {
                                                info!(
                                                            "Updated significance score for cluster {}: {:.4}",
                                                            cluster_id, score
                                                        );
                                            }
                                        }
                                        Err(e) => {
                                            error!(
                                                "Failed to generate summary for cluster {}: {}",
                                                cluster_id, e
                                            );
                                        }
                                    }

                                    // Check for potential cluster merges
                                    let merge_start = Instant::now();
                                    match crate::clustering::check_and_merge_similar_clusters(
                                        db,
                                        cluster_id,
                                        &llm_params.base.llm_client,
                                    )
                                    .await
                                    {
                                        Ok(Some(new_cluster_id)) => {
                                            info!(
                                                "Merged cluster {} into new cluster {} in {:?}",
                                                cluster_id,
                                                new_cluster_id,
                                                merge_start.elapsed()
                                            );
                                        }
                                        Ok(None) => {
                                            debug!("No clusters merged for cluster {}", cluster_id);
                                        }
                                        Err(e) => {
                                            error!("Error checking for cluster merges: {}", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to assign article {} to cluster: {}", article_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to process entity extraction: {:?}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to extract entities: {:?}", e);
            }
        }

        // Get event date
        let (_, event_date) = db
            .get_article_details_with_dates(article_id)
            .await
            .unwrap_or((None, None));

        // Try to get similar articles with both vector and entity matching
        if let Ok(similar_articles) = get_similar_articles_with_entities(
            &embedding,
            10,
            entity_ids.as_deref(),
            event_date.as_deref(),
            Some(article_id),
        )
        .await
        {
            let mut similar_articles_with_details = Vec::new();
            for article in similar_articles {
                if let Ok(Some((json_url, title, tiny_summary))) =
                    db.get_article_details_by_id(article.id).await
                {
                    similar_articles_with_details.push(build_similar_article_json(
                        &article,
                        Some(json_url),
                        title,
                        Some(tiny_summary),
                    ));
                } else {
                    // Include basic info if details can't be fetched
                    similar_articles_with_details
                        .push(build_similar_article_json(&article, None, None, None));
                }
            }
            response_json["similar_articles"] = json!(similar_articles_with_details);
        } else if let Ok(similar_articles) = get_similar_articles(&embedding, 10).await {
            // Fallback to regular vector similarity if entity-aware search fails
            let mut similar_articles_with_details = Vec::new();
            for article in similar_articles {
                if let Ok(Some((json_url, title, tiny_summary))) =
                    db.get_article_details_by_id(article.id).await
                {
                    // Use our helper but then add the fallback formula
                    let mut json_obj = build_similar_article_json(
                        &article,
                        Some(json_url),
                        title,
                        Some(tiny_summary),
                    );
                    json_obj["similarity_formula"] = "Vector similarity only (fallback)".into();
                    similar_articles_with_details.push(json_obj);
                }
            }
            response_json["similar_articles"] = json!(similar_articles_with_details);
        }

        // Store embedding with entity IDs
        if let Err(e) = store_embedding(
            article_id,
            &embedding,
            pub_date,
            topic,
            quality,
            entity_ids,
            event_date.as_deref(),
        )
        .await
        {
            error!("Failed to store vector embedding: {:?}", e);
        }
    }

    Ok(())
}
