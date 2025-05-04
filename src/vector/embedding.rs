use anyhow::Result;
use candle_core::{DType, Tensor};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::Instant;
use tracing::{error, info};

use crate::vector::{
    config::{init_e5_model, init_e5_tokenizer, E5Config},
    TARGET_VECTOR,
};

// Static initialized flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Generate an embedding for a given article text
async fn get_article_embedding(prefixed_text: &str, config: &E5Config) -> Result<Vec<f32>> {
    let start_time = Instant::now();
    let model = crate::vector::model()?;
    let tokenizer = crate::vector::tokenizer()?;

    let tokenize_start = Instant::now();
    let encoding = tokenizer
        .encode(prefixed_text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

    // Truncate to max_length - 1 to avoid index boundary issues
    let max_len = config.max_length - 1;
    let input_ids: Vec<i64> = encoding
        .get_ids()
        .iter()
        .take(max_len)
        .map(|&x| x as i64)
        .collect();
    let attention_mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .take(max_len)
        .map(|&x| x as i64)
        .collect();

    let tokenize_end = Instant::now();
    let inference_start = Instant::now();

    let input_ids = Tensor::new(input_ids, &config.device)?;
    let attention_mask = Tensor::new(attention_mask, &config.device)?;

    let input_ids = input_ids.unsqueeze(0)?;
    let attention_mask = attention_mask.unsqueeze(0)?;

    // Get the last hidden state
    let hidden_state = model.forward(&input_ids, &attention_mask, None)?;

    info!(target: TARGET_VECTOR, "Shape of hidden_state: {:?}", hidden_state.shape());

    // Convert attention mask to float
    let attention_mask_float = attention_mask.to_dtype(DType::F32)?;
    info!(target: TARGET_VECTOR, "Shape of attention_mask_float: {:?}", attention_mask_float.shape());

    // Expand attention mask for broadcasting (match hidden_state shape)
    let attention_mask_expanded = attention_mask_float
        .unsqueeze(2)?
        .expand(hidden_state.shape())?;
    info!(target: TARGET_VECTOR, "Shape of attention_mask_expanded: {:?}", attention_mask_expanded.shape());

    // Apply attention mask (zero out padding embeddings)
    let masked_hidden = hidden_state.mul(&attention_mask_expanded)?;
    info!(target: TARGET_VECTOR, "Shape of masked_hidden: {:?}", masked_hidden.shape());

    // Sum the masked hidden states along the sequence length dimension
    let summed_hidden = masked_hidden.sum(1)?;
    info!(target: TARGET_VECTOR, "Shape of summed_hidden: {:?}", summed_hidden.shape());

    // Sum the attention mask to count the number of valid tokens
    let valid_token_counts = attention_mask_float
        .sum(1)?
        .unsqueeze(1)?
        .clamp(1.0, f32::MAX)?;
    info!(target: TARGET_VECTOR, "Shape of valid_token_counts: {:?}", valid_token_counts.shape());

    // Perform mean pooling (ensure correct shape for division)
    let valid_token_counts_expanded = valid_token_counts.expand(summed_hidden.shape())?;
    info!(target: TARGET_VECTOR, "Shape of valid_token_counts_expanded: {:?}", valid_token_counts_expanded.shape());
    let mean_pooled = summed_hidden.div(&valid_token_counts_expanded)?;

    // Normalize the vector
    let norm = mean_pooled.sqr()?.sum(1)?.sqrt()?.unsqueeze(1)?;
    let norm_expanded = norm.expand(mean_pooled.shape())?; // [1, 1024]
    let normalized = mean_pooled.div(&norm_expanded)?;

    // Get final vector
    let vector = normalized.squeeze(0)?.to_vec1::<f32>()?;

    let end_time = Instant::now();

    // Calculate statistical properties of the embedding
    let sum: f32 = vector.iter().sum();
    let mean = sum / vector.len() as f32;
    let variance: f32 =
        vector.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / vector.len() as f32;
    let std_dev = variance.sqrt();
    let max = vector.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let min = vector.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let active_dimensions = vector.iter().filter(|&&x| x > mean).count();
    let magnitude: f32 = vector.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();

    info!(target: TARGET_VECTOR,
        "Embedding generation successful: Timing: Input length: {} tokens; Tokenization time: {:?}; Inference time: {:?}; Total time: {:?} - Statistics: Dimensions: {}; Mean: {:.4}; Std Dev: {:.4}; Min: {:.4}; Max: {:.4}; Active dimensions: {}/{} ({:.1}%); Vector magnitude: {:.6}; Original text length: {} chars",
        input_ids.dims()[1],
        tokenize_end.duration_since(tokenize_start),
        end_time.duration_since(inference_start),
        end_time.duration_since(start_time),
        vector.len(),
        mean,
        std_dev,
        min,
        max,
        active_dimensions,
        vector.len(),
        (active_dimensions as f32 / vector.len() as f32) * 100.0,
        magnitude,
        prefixed_text.len()
    );

    Ok(vector)
}

/// Public function to get vector embedding for article text
pub async fn get_article_vectors(text: &str) -> Result<Option<Vec<f32>>> {
    let config = E5Config::default();
    let total_start = Instant::now();

    if !INITIALIZED.load(Ordering::Relaxed) {
        let init_start = Instant::now();
        config.ensure_models_exist().await?;
        let model_init_start = Instant::now();
        init_e5_model(&config)?;
        let tokenizer_init_start = Instant::now();
        init_e5_tokenizer(&config)?;
        INITIALIZED.store(true, Ordering::Relaxed);

        info!(target: TARGET_VECTOR,
            "Initialization timing: Model download/check: {:?}; Model initialization: {:?}; Tokenizer initialization: {:?}; Total init time: {:?}",
            model_init_start.duration_since(init_start),
            tokenizer_init_start.duration_since(model_init_start),
            total_start.duration_since(tokenizer_init_start),
            total_start.duration_since(init_start)
        );
    }

    // Use query-focused embedding to direct the model to focus on event identification
    let prefixed_text = format!(
        "query: What is the main event described in this article? passage: {}",
        text
    );
    match get_article_embedding(&prefixed_text, &config).await {
        Ok(embedding) => {
            let validation_start = Instant::now();

            // Basic validation
            if embedding.len() != config.dimensions {
                error!(target: TARGET_VECTOR, "Unexpected embedding dimensions: got {}, expected {}", 
                    embedding.len(), config.dimensions);
                return Ok(None);
            }

            // Statistical calculations
            let stats_start = Instant::now();

            let sum: f32 = embedding.iter().sum();
            let mean = sum / embedding.len() as f32;
            let variance: f32 =
                embedding.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / embedding.len() as f32;
            let std_dev = variance.sqrt();
            let max = embedding.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            let min = embedding.iter().fold(f32::INFINITY, |a, &b| a.min(b));

            let active_dimensions = embedding.iter().filter(|&&x| x > mean).count();

            let magnitude: f32 = embedding.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();

            let end_time = Instant::now();

            info!(target: TARGET_VECTOR, "Embedding generation complete: Timing: Embedding generation: {:?}; Validation: {:?}; Statistics calculation: {:?}; Total processing time: {:?} - Statistics: Dimensions: {}; Mean: {:.4}; Std Dev: {:.4}; Min: {:.4}; Max: {:.4}; Active dimensions: {}/{} ({:.1}%); Vector magnitude: {:.6}",
                validation_start.duration_since(total_start),
                stats_start.duration_since(validation_start),
                end_time.duration_since(stats_start),
                end_time.duration_since(total_start),
                embedding.len(),
                mean,
                std_dev,
                min,
                max,
                active_dimensions,
                embedding.len(),
                (active_dimensions as f32 / embedding.len() as f32) * 100.0,
                magnitude
            );

            Ok(Some(embedding))
        }
        Err(e) => {
            let end_time = Instant::now();
            error!(target: TARGET_VECTOR,
                "Failed to generate embedding after {:?}: {:?}",
                end_time.duration_since(total_start),
                e
            );
            Ok(None)
        }
    }
}
