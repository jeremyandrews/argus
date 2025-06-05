use anyhow::Result;
use argus::db::core::Database;
use chrono::Utc;
use sqlx::{self, Row};
use tracing::{error, info, warn};

/// Repairs missing cluster mappings for articles that have cluster_id but no mapping
///
/// This script fixes the bug where articles were assigned to clusters but the
/// article_cluster_mappings table entries were never created, preventing
/// cluster summaries from being generated.
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let db = Database::instance().await;

    info!("🔧 Starting cluster mapping repair process...");

    // Find articles that have cluster_id but no mapping
    let broken_articles = sqlx::query(
        r#"
        SELECT a.id, a.cluster_id
        FROM articles a
        WHERE a.cluster_id IS NOT NULL
        AND NOT EXISTS (
            SELECT 1 FROM article_cluster_mappings acm 
            WHERE acm.article_id = a.id
        )
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    if broken_articles.is_empty() {
        info!("✅ No broken cluster mappings found. All articles are properly mapped.");
        return Ok(());
    }

    info!(
        "🚨 Found {} articles with missing cluster mappings",
        broken_articles.len()
    );

    let mut repaired_count = 0;
    let mut failed_count = 0;

    for row in broken_articles {
        let article_id: i64 = row.get("id");
        let cluster_id: i64 = row.get("cluster_id");

        match repair_article_mapping(&db, article_id, cluster_id).await {
            Ok(()) => {
                repaired_count += 1;
                info!(
                    "✅ Repaired mapping for article {} → cluster {}",
                    article_id, cluster_id
                );
            }
            Err(e) => {
                failed_count += 1;
                error!(
                    "❌ Failed to repair article {} → cluster {}: {}",
                    article_id, cluster_id, e
                );
            }
        }
    }

    info!("🏁 Repair process completed:");
    info!("   ✅ Successfully repaired: {}", repaired_count);

    if failed_count > 0 {
        warn!("   ❌ Failed repairs: {}", failed_count);
    }

    if repaired_count > 0 {
        info!("🎯 Cluster summaries should now generate automatically for repaired clusters");
        info!("💡 Monitor analysis workers to see cluster summary generation progress");
    }

    Ok(())
}

/// Repairs the mapping for a single article
async fn repair_article_mapping(db: &Database, article_id: i64, cluster_id: i64) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    // Check if cluster still exists
    let cluster_exists = sqlx::query("SELECT 1 FROM article_clusters WHERE id = ?")
        .bind(cluster_id)
        .fetch_optional(db.pool())
        .await?
        .is_some();

    if !cluster_exists {
        return Err(anyhow::anyhow!(
            "Cluster {} no longer exists for article {}",
            cluster_id,
            article_id
        ));
    }

    // Create the missing mapping with similarity score 1.0
    // (founding articles get perfect similarity)
    sqlx::query(
        r#"
        INSERT INTO article_cluster_mappings
        (article_id, cluster_id, added_date, similarity_score)
        VALUES (?, ?, ?, 1.0)
        "#,
    )
    .bind(article_id)
    .bind(cluster_id)
    .bind(&now)
    .execute(db.pool())
    .await?;

    // Ensure cluster is flagged for summary update
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET needs_summary_update = 1
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    Ok(())
}
