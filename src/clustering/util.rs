use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Creates an empty cluster with default values
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the newly created cluster
/// * `Err` - If there was an error during creation
pub async fn create_empty_cluster(db: &Database) -> Result<i64> {
    cluster::create_empty_cluster(db).await
}
