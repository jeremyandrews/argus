use anyhow::Result;

use crate::db::cluster;
use crate::db::core::Database;

/// Check if two clusters are temporally close (have articles published in similar timeframes)
pub async fn are_clusters_temporally_close(
    db: &Database,
    cluster_id1: i64,
    cluster_id2: i64,
    max_days_apart: i32,
) -> Result<bool> {
    // Get the most recent article date for each cluster
    let date1 = cluster::get_most_recent_article_date(db, cluster_id1).await?;
    let date2 = cluster::get_most_recent_article_date(db, cluster_id2).await?;

    match (date1, date2) {
        (Some(date1), Some(date2)) => {
            // Parse the dates
            let dt1 = chrono::DateTime::parse_from_rfc3339(&date1)?;
            let dt2 = chrono::DateTime::parse_from_rfc3339(&date2)?;

            // Calculate days difference
            let diff = (dt1.timestamp() - dt2.timestamp()).abs() as f64 / 86400.0;
            Ok(diff <= max_days_apart as f64)
        }
        _ => Ok(false), // If we can't get dates, assume not temporally close
    }
}
