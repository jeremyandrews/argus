use anyhow::Result;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel, Config as BertConfig, HiddenAct, PositionEmbeddingType,
};
use once_cell::sync::OnceCell;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{
    CreateCollection, Distance, PointId, PointStruct, UpsertPoints, Vector, VectorParams, Vectors,
    VectorsConfig, WriteOrdering,
};
use qdrant_client::Qdrant;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tokio::fs;
use tokio::time::Instant;
use tracing::{error, info};

// Static globals
static MODEL: OnceCell<Arc<BertModel>> = OnceCell::new();
static TOKENIZER: OnceCell<Arc<Tokenizer>> = OnceCell::new();

const MODEL_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/model.safetensors";
const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/tokenizer.json";
const TARGET_VECTOR: &str = "vector";
const QDRANT_URL_ENV: &str = "QDRANT_URL";

/**
* Created Qdrant schema:
* $ curl -X PUT 'http://qdrant:6333/collections/articles' \
*     -H 'Content-Type: application/json' \
*     -d '{
*     "name": "articles",
*     "vectors": {
*         "size": 1024,
*         "distance": "Cosine"
*     },
*     "payload_schema": {
*         "sqlite_id": "integer"
*     },
*     "optimizers_config": {
*         "indexing_threshold": 20000
*     },
*     "hnsw_config": {
*         "m": 16,
*         "ef_construct": 100,
*         "full_scan_threshold": 10000
*     },
*     "points_count": 0,
*     "on_disk_payload": true,
*     "points_id_type": "numeric"
* }'
*/

struct E5Config {
    model_path: String,
    tokenizer_path: String,
    dimensions: usize,
    max_length: usize,
    _similarity_threshold: f32,
    device: Device,
}

impl Default for E5Config {
    fn default() -> Self {
        Self {
            model_path: "models/e5-large-v2.safetensors".to_string(),
            tokenizer_path: "models/e5-tokenizer.json".to_string(),
            dimensions: 1024,
            max_length: 512,
            _similarity_threshold: 0.85,
            device: Device::Cpu,
        }
    }
}

impl E5Config {
    async fn ensure_models_exist(&self) -> Result<()> {
        // Create models directory if it doesn't exist
        if !Path::new("models").exists() {
            fs::create_dir("models").await?;
        }

        // Check and download model file if needed
        if !Path::new(&self.model_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 model from {}", MODEL_URL);
            let response = reqwest::get(MODEL_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.model_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 model to {}", self.model_path);
        }

        // Check and download tokenizer file if needed
        if !Path::new(&self.tokenizer_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 tokenizer from {}", TOKENIZER_URL);
            let response = reqwest::get(TOKENIZER_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.tokenizer_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 tokenizer to {}", self.tokenizer_path);
        }

        Ok(())
    }
}

fn init_e5_model(config: &E5Config) -> Result<()> {
    info!(target: TARGET_VECTOR, "Starting to load E5 model from {}", config.model_path);
    let bert_config = BertConfig {
        hidden_size: config.dimensions,
        intermediate_size: 4096,
        max_position_embeddings: config.max_length,
        num_attention_heads: 16,
        num_hidden_layers: 24,
        vocab_size: 30522,
        layer_norm_eps: 1e-12,
        pad_token_id: 0,
        hidden_act: HiddenAct::Gelu,
        hidden_dropout_prob: 0.0,
        type_vocab_size: 2,
        initializer_range: 0.02,
        position_embedding_type: PositionEmbeddingType::Absolute,
        use_cache: false,
        classifier_dropout: None,
        model_type: None,
    };

    // Load the safetensors file
    let tensors = match candle_core::safetensors::load_buffer(
        &std::fs::read(&config.model_path)?,
        &config.device,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load model tensors: {}", e);
            return Err(anyhow::anyhow!("Failed to load model tensors"));
        }
    };

    // Create VarBuilder from the loaded tensors
    let vb = VarBuilder::from_tensors(tensors, DType::F32, &config.device);

    // Load the model
    let model = match BertModel::load(vb, &bert_config) {
        Ok(m) => m,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load BERT model: {}", e);
            return Err(anyhow::anyhow!("Failed to load BERT model"));
        }
    };

    // Set the model in the static
    if MODEL.set(Arc::new(model)).is_err() {
        error!(target: TARGET_VECTOR, "!!! Failed to set model in static");
        return Err(anyhow::anyhow!("Failed to set model in static"));
    }

    info!(target: TARGET_VECTOR, "Successfully loaded E5 model");
    Ok(())
}

fn init_e5_tokenizer(config: &E5Config) -> Result<()> {
    info!(target: TARGET_VECTOR, "Starting to load E5 tokenizer from {}", config.tokenizer_path);

    let tokenizer = match Tokenizer::from_file(&config.tokenizer_path) {
        Ok(t) => t,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load tokenizer: {}", e);
            return Err(anyhow::anyhow!("Failed to load tokenizer"));
        }
    };

    if TOKENIZER.set(Arc::new(tokenizer)).is_err() {
        error!(target: TARGET_VECTOR, "!!! Failed to set tokenizer in static");
        return Err(anyhow::anyhow!("Failed to set tokenizer in static"));
    }

    info!(target: TARGET_VECTOR, "Successfully loaded E5 tokenizer");
    Ok(())
}

async fn get_article_embedding(text: &str, config: &E5Config) -> Result<Vec<f32>> {
    let start_time = Instant::now();
    let model = MODEL.get().expect("E5 model not initialized");
    let tokenizer = TOKENIZER.get().expect("E5 tokenizer not initialized");

    let tokenize_start = Instant::now();
    let prefixed_text = format!("passage: {}", text);
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
        text.len()
    );

    Ok(vector)
}

pub async fn get_article_vectors(text: &str) -> Result<Option<Vec<f32>>> {
    let config = E5Config::default();
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

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

    match get_article_embedding(text, &config).await {
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

pub async fn store_embedding(sqlite_id: i64, embedding: Vec<f32>) -> Result<()> {
    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    let mut payload: HashMap<String, qdrant_client::qdrant::Value> = HashMap::new();
    payload.insert(
        "sqlite_id".to_string(),
        json!(sqlite_id).try_into().unwrap(),
    );

    let point = PointStruct {
        id: Some(PointId {
            point_id_options: Some(PointIdOptions::Num(
                sqlite_id
                    .try_into()
                    .expect("SQLite ID should never be negative"),
            )),
        }),
        vectors: Some(Vectors::from(Vector::new_dense(embedding))),
        payload,
        ..Default::default()
    };

    let upsert_points = UpsertPoints {
        collection_name: "articles".to_string(),
        points: vec![point],
        wait: Some(true),
        ordering: Some(WriteOrdering::default()),
        shard_key_selector: None,
    };

    match client.upsert_points(upsert_points).await {
        Ok(_) => {
            info!(
                target: TARGET_VECTOR,
                "Successfully stored embedding for article {}", sqlite_id
            );
            Ok(())
        }
        Err(e) => {
            error!(
                target: TARGET_VECTOR,
                "Failed to store embedding for article {}: {:?}", sqlite_id, e
            );
            Err(anyhow::anyhow!("Failed to store embedding: {:?}", e))
        }
    }
}

async fn _create_collection() -> Result<()> {
    let qdrant_url =
        std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required");
    let client = Qdrant::from_url(&qdrant_url)
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let vector_params = VectorParams {
        size: 1024, // E5 embedding size
        distance: Distance::Cosine as i32,
        ..Default::default()
    };

    let create_collection = CreateCollection {
        collection_name: "articles".to_string(),
        vectors_config: Some(VectorsConfig {
            config: Some(Config::Params(vector_params)),
        }),
        ..Default::default()
    };

    client.create_collection(create_collection).await?;

    Ok(())
}
