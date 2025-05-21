use crate::db::core::Database;
use crate::entity::extraction::extract_entities;
use crate::{JsonLLMParams, WorkerDetail};
use tokio::time::Instant;
use tracing::{debug, error, info};

/// Extracts entities from the article text and processes them.
/// Returns the IDs of the entities found, if any.
pub async fn process_entities(
    db: &Database,
    article_id: i64,
    article_text: &str,
    pub_date: Option<&str>,
    json_params: &JsonLLMParams,
    worker_detail: &WorkerDetail,
) -> Option<Vec<i64>> {
    let entity_extraction_start = Instant::now();

    match extract_entities(article_text, pub_date, json_params, worker_detail).await {
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
                        article_id,
                        ids.len()
                    );

                    // Process potential aliases
                    process_potential_aliases(db, article_text).await;

                    Some(ids)
                }
                Err(e) => {
                    error!("Failed to process entity extraction: {:?}", e);
                    None
                }
            }
        }
        Err(e) => {
            error!("Failed to extract entities: {:?}", e);
            None
        }
    }
}

/// Processes potential aliases found in the article text
pub async fn process_potential_aliases(db: &Database, article_text: &str) {
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
}

/// Assigns the article to a cluster and generates a summary for that cluster
pub async fn process_article_clustering(
    db: &Database,
    article_id: i64,
    llm_client: &crate::LLMClient,
) {
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
                match crate::clustering::generate_cluster_summary(db, llm_client, cluster_id).await
                {
                    Ok(summary) => {
                        info!(
                            "Generated summary for cluster {} (length: {})",
                            cluster_id,
                            summary.len()
                        );

                        // Update cluster significance
                        if let Ok(score) =
                            crate::clustering::calculate_cluster_significance(db, cluster_id).await
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
            }
        }
        Err(e) => {
            error!("Failed to assign article {} to cluster: {}", article_id, e);
        }
    }
}
