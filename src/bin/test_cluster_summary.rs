use anyhow::Result;
use argus::clustering::{generate_cluster_summary, get_clusters_needing_summary_updates};
use argus::db::core::Database;
use argus::vector::get_default_llm_client;
use argus::DEFAULT_OLLAMA_MODEL;
use std::env;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    info!("🧪 Testing cluster summary generation...");

    // Get database instance
    let db = Database::instance().await;

    // Get clusters that need summary updates
    match get_clusters_needing_summary_updates(&db).await {
        Ok(cluster_ids) => {
            info!(
                "📊 Found {} clusters needing summary updates",
                cluster_ids.len()
            );

            if cluster_ids.is_empty() {
                info!("✅ No clusters need summary updates");
                return Ok(());
            }

            // Test with the first cluster
            let test_cluster_id = cluster_ids[0];
            info!(
                "🎯 Testing cluster summary generation for cluster {}",
                test_cluster_id
            );

            // Get LLM client and model
            let llm_client = get_default_llm_client();
            let model_name =
                env::var("LLM_MODEL").unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());

            info!("🤖 Using model: {}", model_name);

            // Generate summary
            match generate_cluster_summary(&db, &llm_client, test_cluster_id, &model_name, None)
                .await
            {
                Ok(summary) => {
                    info!("✅ Successfully generated cluster summary!");
                    info!("📄 Summary length: {} characters", summary.len());
                    info!(
                        "📄 Summary preview: {}",
                        if summary.len() > 200 {
                            format!("{}...", &summary[..200])
                        } else {
                            summary.clone()
                        }
                    );

                    // Test a few more clusters
                    for &cluster_id in cluster_ids.iter().take(3).skip(1) {
                        info!("🎯 Testing cluster {}", cluster_id);
                        match generate_cluster_summary(
                            &db,
                            &llm_client,
                            cluster_id,
                            &model_name,
                            None,
                        )
                        .await
                        {
                            Ok(summary) => {
                                info!(
                                    "✅ Cluster {} summary generated ({} chars)",
                                    cluster_id,
                                    summary.len()
                                );
                            }
                            Err(e) => {
                                error!(
                                    "❌ Failed to generate summary for cluster {}: {}",
                                    cluster_id, e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to generate cluster summary: {}", e);

                    // Enhanced debugging information
                    info!("🔍 Debug information:");
                    info!("  - Model: {}", model_name);
                    info!("  - Cluster ID: {}", test_cluster_id);

                    // Check if the cluster has articles
                    match argus::db::cluster::get_cluster_articles(&db, test_cluster_id, 10).await {
                        Ok(articles) => {
                            info!("  - Articles in cluster: {}", articles.len());
                            for (i, article) in articles.iter().take(3).enumerate() {
                                info!(
                                    "    Article {}: {} (Quality: {})",
                                    i + 1,
                                    article.title.as_deref().unwrap_or("No title"),
                                    article.quality_score
                                );
                            }
                        }
                        Err(e) => {
                            error!("  - Failed to get cluster articles: {}", e);
                        }
                    }

                    // Check cluster entities
                    match argus::db::cluster::get_cluster_entity_details(&db, test_cluster_id).await
                    {
                        Ok(entities) => {
                            info!("  - Entities in cluster: {}", entities.len());
                        }
                        Err(e) => {
                            error!("  - Failed to get cluster entities: {}", e);
                        }
                    }

                    return Err(e);
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to get clusters needing updates: {}", e);
            return Err(e);
        }
    }

    info!("🎉 Cluster summary test completed!");
    Ok(())
}
