use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Combine all entity IDs from multiple clusters
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_ids` - IDs of clusters to combine entities from
///
/// # Returns
/// * `Ok(Vec<i64>)` - Combined unique entity IDs
/// * `Err` - If there was an error during processing
pub async fn combine_entities_from_clusters(
    db: &Database,
    cluster_ids: &[i64],
) -> Result<Vec<i64>> {
    cluster::combine_entities_from_clusters(db, cluster_ids).await
}

/// Updates a cluster's primary entities
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to update
/// * `entity_ids` - New primary entity IDs
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
pub async fn update_cluster_primary_entities(
    db: &Database,
    cluster_id: i64,
    entity_ids: &[i64],
) -> Result<()> {
    cluster::update_cluster_primary_entities(db, cluster_id, entity_ids).await
}
