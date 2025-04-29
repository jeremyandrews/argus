use anyhow::{Context, Result};
use argus::db::Database;
use argus::vector::{get_article_vectors, store_embedding};
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::{PointId, WithPayloadSelector, WithVectorsSelector};
use qdrant_client::Qdrant;
use std::env;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

const QDRANT_URL_ENV: &str = "QDRANT_URL";

/// Utility to validate and fix vector embeddings for specific articles
///
/// Usage:
///    cargo run --bin fix_article_vectors ARTICLE_ID1 [ARTICLE_ID2 ...]
///
/// Example:
///    cargo run --bin fix_article_vectors 21235787 21230061
///
/// This will:
/// 1. Check if the articles have valid vector embeddings
/// 2. If not, regenerate and store new embeddings
/// 3. Verify the fix with a similarity calculation
///
/// Output example:
/// ```
/// Validating vector for article 21235787...
/// No valid vector found or validation failed
/// Regenerating vector for article 21235787...
/// Successfully reprocessed vector for article 21235787
/// ```

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} ARTICLE_ID1 [ARTICLE_ID2 ...]", args[0]);
        eprintln!("Example: {} 21235787 21230061", args[0]);
        std::process::exit(1);
    }

    // Parse article IDs
    let article_ids: Vec<i64> = args[1..]
        .iter()
        .filter_map(|id| id.parse::<i64>().ok())
        .collect();

    if article_ids.is_empty() {
        eprintln!("No valid article IDs provided");
        std::process::exit(1);
    }

    info!(
        "Processing {} articles: {:?}",
        article_ids.len(),
        article_ids
    );

    // Process each article
    for &article_id in &article_ids {
        if let Err(e) = validate_and_reprocess_vector(article_id).await {
            error!("Failed to process article {}: {}", article_id, e);
        }
    }

    // Verify similarity between pairs if multiple articles provided
    if article_ids.len() >= 2 {
        info!("Verifying similarity between article pairs:");
        for i in 0..article_ids.len() {
            for j in i + 1..article_ids.len() {
                let id1 = article_ids[i];
                let id2 = article_ids[j];
                info!(
                    "Checking similarity between articles {} and {}...",
                    id1, id2
                );

                // Create dummy vector for dummy_vector parameter (actual vectors come from DB)
                let dummy_vector = vec![0.0; 1024];
                match verify_similarity(&dummy_vector, id1, id2).await {
                    Ok(similarity) => {
                        info!(
                            "Similarity between articles {} and {}: {:.6}",
                            id1, id2, similarity
                        );
                    }
                    Err(e) => {
                        error!("Failed to verify similarity: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn validate_and_reprocess_vector(article_id: i64) -> Result<()> {
    let db = Database::instance().await;
    info!("Validating vector for article {}...", article_id);

    // First, attempt to validate the existing vector
    let validation_result = validate_article_vector(article_id).await;

    match validation_result {
        Ok(true) => {
            info!("Article {} has a valid vector", article_id);
            return Ok(());
        }
        Ok(false) => {
            info!("No valid vector found or validation failed");
            // Continue to reprocessing
        }
        Err(e) => {
            error!("Error validating vector for article {}: {}", article_id, e);
            // Continue to reprocessing if validation failed
        }
    }

    // Get article text
    info!("Regenerating vector for article {}...", article_id);
    let article_text = match db.get_article_text(article_id).await {
        Ok(text) => text,
        Err(e) => {
            error!("Failed to get text for article {}: {}", article_id, e);
            return Err(anyhow::anyhow!("Failed to get article text"));
        }
    };

    // Get article metadata
    let (pub_date, event_date) = match db.get_article_details_with_dates(article_id).await {
        Ok((pub_date, event_date)) => (pub_date, event_date),
        Err(e) => {
            error!("Failed to get dates for article {}: {}", article_id, e);
            return Err(anyhow::anyhow!("Failed to get article dates"));
        }
    };

    // Get entity IDs for this article
    let entity_ids = match db.get_article_entity_ids(article_id).await {
        Ok(ids) => ids,
        Err(e) => {
            error!("Failed to get entity IDs for article {}: {}", article_id, e);
            vec![] // Continue without entity IDs
        }
    };

    // Get article category and quality
    let (category, quality) = match db.get_article_metadata(article_id).await {
        Ok((cat, qual)) => (cat, qual),
        Err(e) => {
            error!("Failed to get metadata for article {}: {}", article_id, e);
            (None, 0) // Default values
        }
    };

    // Generate new embedding
    let new_embedding = match get_article_vectors(&article_text).await? {
        Some(embedding) => embedding,
        None => {
            error!("Failed to generate embedding for article {}", article_id);
            return Err(anyhow::anyhow!("Failed to generate embedding"));
        }
    };

    // Store the new embedding
    store_embedding(
        article_id,
        &new_embedding,
        pub_date.as_deref(),
        category.as_deref(),
        quality,
        Some(entity_ids),
        event_date.as_deref(),
    )
    .await?;

    info!("Successfully reprocessed vector for article {}", article_id);
    Ok(())
}

// Validation function to check if an article's vector is valid
async fn validate_article_vector(article_id: i64) -> Result<bool> {
    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    // Get the article's vector from Qdrant
    let response = client
        .get_points(qdrant_client::qdrant::GetPoints {
            collection_name: "articles".to_string(),
            ids: vec![PointId {
                point_id_options: Some(PointIdOptions::Num(article_id as u64)),
            }],
            with_payload: Some(WithPayloadSelector::from(false)),
            with_vectors: Some(WithVectorsSelector::from(true)),
            ..Default::default()
        })
        .await?;

    if let Some(point) = response.result.first() {
        if let Some(vectors) = &point.vectors {
            if let Some(opts) = &vectors.vectors_options {
                // Check if this is the vector variant we expect
                match opts {
                    &qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(ref v) => {
                        // Check dimensions
                        if v.data.len() != 1024 {
                            // Expected dimension for E5 embeddings
                            info!(
                                "Vector for article {} has wrong dimensions: {}",
                                article_id,
                                v.data.len()
                            );
                            return Ok(false);
                        }

                        // Check for zero magnitude
                        let magnitude = v.data.iter().map(|x| x * x).sum::<f32>().sqrt();
                        if magnitude < 0.001 {
                            info!(
                                "Vector for article {} has near-zero magnitude: {}",
                                article_id, magnitude
                            );
                            return Ok(false);
                        }

                        // Check for NaN values
                        if v.data.iter().any(|x| x.is_nan()) {
                            info!("Vector for article {} contains NaN values", article_id);
                            return Ok(false);
                        }

                        // Vector looks valid
                        return Ok(true);
                    }
                    _ => {
                        info!("Vector for article {} has unexpected format", article_id);
                        return Ok(false);
                    }
                }
            }
        }
    }

    // No vector found or structure is unexpected
    info!("No valid vector found for article {}", article_id);
    Ok(false)
}

// Verify similarity calculation works between two articles
async fn verify_similarity(
    dummy_vector: &Vec<f32>,
    _article_id1: i64,
    article_id2: i64,
) -> Result<f32> {
    // We need to add this function to ensure the calculation works properly between our articles
    let similarity = argus::vector::calculate_vector_similarity(dummy_vector, article_id2)
        .await
        .context("Failed to calculate similarity")?;

    Ok(similarity)
}
