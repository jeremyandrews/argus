use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Calculates the importance score for a cluster
///
/// The score is based on:
/// - Number of articles (more articles = higher score)
/// - Average quality of articles (higher quality = higher score)
/// - Recency of updates (more recent = higher score)
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
///
/// # Returns
/// * `Ok(f64)` - The calculated importance score
/// * `Err` - If there was an error during calculation
pub async fn calculate_cluster_significance(db: &Database, cluster_id: i64) -> Result<f64> {
    cluster::calculate_cluster_significance(db, cluster_id).await
}
