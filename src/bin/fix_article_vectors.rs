use anyhow::Result;
use argus::db::Database;
use argus::vector::{get_article_vectors, store_embedding};
use std::env;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

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

                match verify_similarity(id1, id2).await {
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
    match argus::get_article_vector_from_qdrant(article_id).await {
        Ok(_) => {
            // Vector was successfully retrieved and validated
            info!("Article {} has a valid vector", article_id);
            Ok(true)
        }
        Err(e) => {
            info!("Vector validation failed for article {}: {}", article_id, e);
            Ok(false)
        }
    }
}

// Verify similarity calculation works between two articles
async fn verify_similarity(article_id1: i64, article_id2: i64) -> Result<f32> {
    // Get both vectors
    let vec1 = argus::get_article_vector_from_qdrant(article_id1).await?;
    let vec2 = argus::get_article_vector_from_qdrant(article_id2).await?;

    // Calculate direct similarity
    let similarity = argus::calculate_direct_similarity(&vec1, &vec2)?;

    Ok(similarity)
}
