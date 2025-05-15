use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Marks a cluster as merged into another cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to mark as merged
/// * `merged_into` - ID of the cluster it was merged into
/// * `reason` - Optional reason for the merge
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
pub async fn mark_cluster_as_merged(
    db: &Database,
    cluster_id: i64,
    merged_into: i64,
    reason: Option<&str>,
) -> Result<()> {
    cluster::mark_cluster_as_merged(db, cluster_id, merged_into, reason).await
}

/// Get all clusters that have been merged and their destinations
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(Vec<(i64, i64)>)` - Vector of (original_id, merged_into_id) pairs
/// * `Err` - If there was an error during retrieval
pub async fn get_merged_clusters(db: &Database) -> Result<Vec<(i64, i64)>> {
    cluster::get_merged_clusters(db).await
}

/// Checks if a cluster has been merged and returns the current active cluster ID
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to check
///
/// # Returns
/// * `Ok(Some(i64))` - ID of the cluster it was merged into, if applicable
/// * `Ok(None)` - If the cluster is still active
/// * `Err` - If there was an error during the check
pub async fn get_merged_cluster_destination(db: &Database, cluster_id: i64) -> Result<Option<i64>> {
    cluster::get_merged_cluster_destination(db, cluster_id).await
}

/// Gets the source clusters that were merged into this cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the merged cluster
///
/// # Returns
/// * `Ok(Vec<i64>)` - IDs of clusters that were merged into this one
/// * `Err` - If there was an error during retrieval
pub async fn get_clusters_merged_into(db: &Database, cluster_id: i64) -> Result<Vec<i64>> {
    cluster::get_clusters_merged_into(db, cluster_id).await
}
