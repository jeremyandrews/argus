use anyhow::Result;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel, Config as BertConfig, HiddenAct, PositionEmbeddingType,
};
use once_cell::sync::OnceCell;
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
    let bert_config = BertConfig {
        hidden_size: config.dimensions,
        intermediate_size: 4096,
        max_position_embeddings: config.max_length,
        num_attention_heads: 16,
        num_hidden_layers: 24,
        vocab_size: 250000,
        layer_norm_eps: 1e-12,
        pad_token_id: 0,
        hidden_act: HiddenAct::Gelu,
        hidden_dropout_prob: 0.0,
        type_vocab_size: 1,
        initializer_range: 0.02,
        position_embedding_type: PositionEmbeddingType::Absolute,
        use_cache: false,
        classifier_dropout: None,
        model_type: None,
    };

    info!(target: TARGET_VECTOR, "Loading E5 model from {}", config.model_path);

    // Load the safetensors file
    let tensors =
        candle_core::safetensors::load_buffer(&std::fs::read(&config.model_path)?, &config.device)?;

    // Create VarBuilder from the loaded tensors
    let vb = VarBuilder::from_tensors(tensors, DType::F32, &config.device);

    let model = BertModel::load(vb, &bert_config)?;

    MODEL
        .set(Arc::new(model))
        .map_err(|_| anyhow::anyhow!("Failed to set model"))?;
    Ok(())
}

fn init_e5_tokenizer(config: &E5Config) -> Result<()> {
    info!(target: TARGET_VECTOR, "Loading E5 tokenizer from {}", config.tokenizer_path);
    let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;
    TOKENIZER
        .set(Arc::new(tokenizer))
        .map_err(|_| anyhow::anyhow!("Failed to set tokenizer"))?;
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
    let tokenize_end = Instant::now();

    let inference_start = Instant::now();
    let input_ids = Tensor::new(
        encoding
            .get_ids()
            .iter()
            .map(|&x| x as i64)
            .collect::<Vec<_>>(),
        &config.device,
    )?;
    let attention_mask = Tensor::new(
        encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect::<Vec<_>>(),
        &config.device,
    )?;

    let input_ids = input_ids.unsqueeze(0)?;
    let attention_mask = attention_mask.unsqueeze(0)?;

    // Get the last hidden state directly
    let hidden_state = model.forward(&input_ids, &attention_mask, None)?;

    let mask = attention_mask.unsqueeze(2)?;
    let mask = mask.to_dtype(DType::F32)?;
    let masked = hidden_state.mul(&mask)?;
    let summed = masked.sum(1)?;
    let counts = mask.sum(1)?;
    let mean_pooled = summed.div(&counts)?;

    let norm = mean_pooled.sqr()?.sum_all()?.sqrt()?;
    let normalized = mean_pooled.div(&norm)?;

    let vector = normalized.squeeze(0)?.to_vec1::<f32>()?;
    let end_time = Instant::now();

    info!(target: TARGET_VECTOR,
        "Embedding generation timing:\n\
         - Tokenization: {:?}\n\
         - Model inference: {:?}\n\
         - Total time: {:?}\n\
         - Input text length: {} chars\n\
         - Tokens generated: {}",
        tokenize_end.duration_since(tokenize_start),
        end_time.duration_since(inference_start),
        end_time.duration_since(start_time),
        text.len(),
        encoding.get_ids().len()
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
            "Initialization timing: \n\
             - Model download/check: {:?}\n\
             - Model initialization: {:?}\n\
             - Tokenizer initialization: {:?}\n\
             - Total init time: {:?}",
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

            info!(target: TARGET_VECTOR,
                "Embedding generation complete:\n\
                 Timing:\n\
                 - Embedding generation: {:?}\n\
                 - Validation: {:?}\n\
                 - Statistics calculation: {:?}\n\
                 - Total processing time: {:?}\n\
                 \n\
                 Statistics:\n\
                 - Dimensions: {}\n\
                 - Mean: {:.4}\n\
                 - Std Dev: {:.4}\n\
                 - Min: {:.4}\n\
                 - Max: {:.4}\n\
                 - Active dimensions: {}/{} ({:.1}%)\n\
                 - Vector magnitude: {:.6}",
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
