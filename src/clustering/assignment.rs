use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Assigns an article to the most appropriate cluster based on entity overlap
///
/// This function:
/// 1. Retrieves the article's extracted entities
/// 2. Finds the best matching cluster based on entity overlap
/// 3. Creates a new cluster if no good match is found
/// 4. Updates the article with the assigned cluster_id
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article to assign to a cluster
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the cluster the article was assigned to (0 if skipped)
/// * `Err` - If there was an error during the process
pub async fn assign_article_to_cluster(db: &Database, article_id: i64) -> Result<i64> {
    cluster::assign_article_to_cluster(db, article_id).await
}
