use anyhow::Result;
use tracing::{error, info};

use crate::vector::{storage::get_article_vector_from_qdrant, TARGET_VECTOR};

/// Number of days to look back for similar articles
pub const SIMILARITY_TIME_WINDOW_DAYS: i64 = 14;

/// Calculates the date threshold for similarity searches
///
/// # Returns
/// - RFC3339 formatted date string for the threshold (N days ago)
pub fn calculate_similarity_date_threshold() -> String {
    chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(SIMILARITY_TIME_WINDOW_DAYS))
        .unwrap_or_else(|| chrono::Utc::now())
        .to_rfc3339()
}

/// Calculate cosine similarity directly between two vectors
///
/// # Arguments
/// * `vec1` - First vector
/// * `vec2` - Second vector
///
/// # Returns
/// * `Result<f32>` - The cosine similarity or an error
pub fn calculate_direct_similarity(vec1: &[f32], vec2: &[f32]) -> Result<f32> {
    if vec1.len() != vec2.len() {
        return Err(anyhow::anyhow!(
            "Vector dimensions don't match: {} vs {}",
            vec1.len(),
            vec2.len()
        ));
    }

    let mag1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag1 < 0.001 || mag2 < 0.001 {
        return Err(anyhow::anyhow!("Zero magnitude vector detected"));
    }

    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let similarity = dot_product / (mag1 * mag2);

    Ok(similarity)
}

/// Calculate vector similarity between an embedding and a specific article
pub async fn calculate_vector_similarity(embedding: &Vec<f32>, article_id: i64) -> Result<f32> {
    info!(target: TARGET_VECTOR, "Starting vector similarity calculation for article {}", article_id);
    let source_magnitude = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    info!(target: TARGET_VECTOR, "Source embedding dimensions: {}, magnitude: {:.6}", 
          embedding.len(), source_magnitude);

    if source_magnitude < 0.001 {
        error!(target: TARGET_VECTOR, "CRITICAL: Source vector has near-zero magnitude");
        return Err(anyhow::anyhow!("Source vector has near-zero magnitude"));
    }

    // Get the target article's vector using our function
    let target_vector = get_article_vector_from_qdrant(article_id).await?;

    // Check dimensions match
    if embedding.len() != target_vector.len() {
        error!(target: TARGET_VECTOR, "CRITICAL: Vector dimension mismatch for article {}: source={}, target={}", 
               article_id, embedding.len(), target_vector.len());
        return Err(anyhow::anyhow!("Vector dimension mismatch"));
    }

    // Use the direct similarity calculation
    let similarity = calculate_direct_similarity(embedding, &target_vector)?;
    info!(target: TARGET_VECTOR, "Successfully calculated similarity for article {}: {:.6}", article_id, similarity);

    Ok(similarity)
}
