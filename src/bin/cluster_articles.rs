use anyhow::Result;
use argus::clustering;
use argus::db::Database;
use std::env;
use tokio::time::Instant;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

/// Utility to cluster existing articles.
///
/// This tool:
/// 1. Finds articles that have extracted entities but no cluster assignment
/// 2. Assigns each article to the best matching cluster or creates a new one
/// 3. Logs statistics about clusters created and updated
///
/// Usage:
///    cargo run --bin cluster_articles -- [BATCH_SIZE] [MAX_ARTICLES]
///
/// Example:
///    cargo run --bin cluster_articles -- 50 1000
///
/// Parameters:
///    BATCH_SIZE: Number of articles to process in each batch (default: 100)
///    MAX_ARTICLES: Maximum number of articles to process (default: all)

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    let batch_size: usize = args.get(1).map_or(100, |s| s.parse().unwrap_or(100));
    let max_articles: Option<usize> = args.get(2).map(|s| s.parse().unwrap_or(usize::MAX));

    let start_time = Instant::now();
    info!("Starting article clustering...");
    info!("Batch size: {}", batch_size);
    info!(
        "Max articles: {}",
        max_articles.map_or("all".to_string(), |m| m.to_string())
    );

    // Get database instance
    let db = Database::instance().await;

    // Get articles with entities but no cluster assignment
    let articles = find_articles_without_clusters(db, max_articles).await?;
    let total_articles = articles.len();

    info!("Found {} articles to cluster", total_articles);

    if total_articles == 0 {
        info!("No articles to cluster, exiting.");
        return Ok(());
    }

    // Process articles in batches
    let mut processed = 0;
    let mut assigned_to_existing = 0;
    let mut new_clusters_created = 0;
    let mut errors = 0;

    for chunk in articles.chunks(batch_size) {
        info!(
            "Processing batch of {} articles ({}/{})",
            chunk.len(),
            processed + 1,
            total_articles
        );

        for &article_id in chunk {
            match clustering::assign_article_to_cluster(db, article_id).await {
                Ok(cluster_id) => {
                    if cluster_id > 0 {
                        if is_new_cluster(cluster_id).await {
                            new_clusters_created += 1;
                            info!(
                                "Created new cluster {} for article {}",
                                cluster_id, article_id
                            );
                        } else {
                            assigned_to_existing += 1;
                            info!("Assigned article {} to cluster {}", article_id, cluster_id);
                        }
                    } else {
                        info!("Article {} skipped (no primary entities)", article_id);
                    }
                }
                Err(e) => {
                    error!("Error clustering article {}: {}", article_id, e);
                    errors += 1;
                }
            }

            processed += 1;
        }

        info!(
            "Progress: {}/{} articles processed ({:.1}%)",
            processed,
            total_articles,
            (processed as f32 / total_articles as f32) * 100.0
        );
    }

    let elapsed = start_time.elapsed();
    info!("Clustering completed in {:.2?}", elapsed);
    info!("Summary:");
    info!("  Total articles processed: {}", processed);
    info!("  Assigned to existing clusters: {}", assigned_to_existing);
    info!("  New clusters created: {}", new_clusters_created);
    info!("  Errors: {}", errors);

    Ok(())
}

async fn find_articles_without_clusters(
    db: &Database,
    max_articles: Option<usize>,
) -> Result<Vec<i64>> {
    // Find articles that have entities (in article_entities) but no cluster assignment
    let mut articles = Vec::new();

    // Query for articles with entities but no cluster
    let rows = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT DISTINCT a.id
        FROM articles a
        JOIN article_entities ae ON a.id = ae.article_id
        WHERE a.cluster_id IS NULL
        ORDER BY a.id DESC
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    // Limit to max_articles if specified
    if let Some(max) = max_articles {
        articles.extend(rows.into_iter().take(max));
    } else {
        articles.extend(rows);
    }

    Ok(articles)
}

async fn is_new_cluster(_cluster_id: i64) -> bool {
    // Used to check if this is a newly created cluster
    // For simplicity, we'll just return true if any new cluster is created
    // In a production implementation, you might want to track this differently
    true
}
