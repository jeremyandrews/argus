use anyhow::{anyhow, Result};
use tracing::{info, warn};

use crate::clustering::entities::{
    combine_entities_from_clusters, update_cluster_primary_entities,
};
use crate::clustering::merging::history::mark_cluster_as_merged;
use crate::clustering::util::create_empty_cluster;
use crate::db::cluster;
use crate::db::core::Database;
use crate::vector::get_default_llm_client;

/// Merges multiple clusters into a new cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `source_cluster_ids` - IDs of clusters to merge
/// * `reason` - Optional reason for the merge
///
/// # Returns
/// * `Ok(i64)` - ID of the newly created merged cluster
/// * `Err` - If there was an error during the merge
pub async fn merge_clusters(
    db: &Database,
    source_cluster_ids: &[i64],
    reason: Option<&str>,
) -> Result<i64> {
    if source_cluster_ids.len() < 2 {
        return Err(anyhow!("At least two clusters are required for merging"));
    }

    // Step 1: Create new target cluster
    let new_cluster_id = create_empty_cluster(db).await?;
    info!(
        "Created new cluster {} for merge of {} clusters",
        new_cluster_id,
        source_cluster_ids.len()
    );

    // Step 2: Combine entities from all source clusters
    let combined_entities = combine_entities_from_clusters(db, source_cluster_ids).await?;
    update_cluster_primary_entities(db, new_cluster_id, &combined_entities).await?;

    let mut total_articles = 0;

    // Step 3: Update all article mappings
    for &source_id in source_cluster_ids {
        // Get all articles from source cluster
        let articles = cluster::get_cluster_articles(db, source_id, 10000).await?;
        total_articles += articles.len();

        // Create mappings to the new cluster
        for article in articles {
            // Add article to new cluster (preserve original similarity score)
            cluster::assign_to_cluster(db, article.id, new_cluster_id, article.similarity_score)
                .await?;

            // Update article's primary cluster_id reference
            cluster::update_article_cluster_id(db, article.id, new_cluster_id).await?;
        }

        // Mark source cluster as merged
        mark_cluster_as_merged(db, source_id, new_cluster_id, reason).await?;
    }

    // Update article count in the database
    cluster::update_cluster_article_count(db, new_cluster_id, total_articles as i32).await?;

    // Step 5: Update user preferences
    // TODO: Move this to db/cluster.rs module
    // transfer_user_preferences(db, source_cluster_ids, new_cluster_id).await?;

    // Step 6: Generate a new summary
    let _summary = match crate::clustering::summary::generate_cluster_summary(
        db,
        &get_default_llm_client(),
        new_cluster_id,
    )
    .await
    {
        Ok(summary) => summary,
        Err(e) => {
            warn!(
                "Failed to generate summary for merged cluster {}: {}",
                new_cluster_id, e
            );
            String::from(
                "This is a merged cluster containing related articles from multiple topics.",
            )
        }
    };

    // Step 7: Calculate significance score for the new cluster
    let _score =
        match crate::clustering::significance::calculate_cluster_significance(db, new_cluster_id)
            .await
        {
            Ok(score) => score,
            Err(e) => {
                warn!(
                    "Failed to calculate significance for merged cluster {}: {}",
                    new_cluster_id, e
                );
                0.0
            }
        };

    info!(
        "Successfully merged {} clusters into new cluster {} with {} articles",
        source_cluster_ids.len(),
        new_cluster_id,
        total_articles
    );

    Ok(new_cluster_id)
}
