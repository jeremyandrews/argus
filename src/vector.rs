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
use tracing::{error, info};

// Static globals
static MODEL: OnceCell<Arc<BertModel>> = OnceCell::new();
static TOKENIZER: OnceCell<Arc<Tokenizer>> = OnceCell::new();

const MODEL_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/model.safetensors";
const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/tokenizer.json";

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
            info!("Downloading E5 model from {}", MODEL_URL);
            let response = reqwest::get(MODEL_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.model_path, bytes).await?;
            info!("Downloaded E5 model to {}", self.model_path);
        }

        // Check and download tokenizer file if needed
        if !Path::new(&self.tokenizer_path).exists() {
            info!("Downloading E5 tokenizer from {}", TOKENIZER_URL);
            let response = reqwest::get(TOKENIZER_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.tokenizer_path, bytes).await?;
            info!("Downloaded E5 tokenizer to {}", self.tokenizer_path);
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

    info!("Loading E5 model from {}", config.model_path);

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
    info!("Loading E5 tokenizer from {}", config.tokenizer_path);
    let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;
    TOKENIZER
        .set(Arc::new(tokenizer))
        .map_err(|_| anyhow::anyhow!("Failed to set tokenizer"))?;
    Ok(())
}

async fn get_article_embedding(text: &str, config: &E5Config) -> Result<Vec<f32>> {
    let model = MODEL.get().expect("E5 model not initialized");
    let tokenizer = TOKENIZER.get().expect("E5 tokenizer not initialized");

    let prefixed_text = format!("passage: {}", text);

    let encoding = tokenizer
        .encode(prefixed_text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

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

    Ok(vector)
}

pub async fn get_article_vectors(text: &str) -> Result<Option<Vec<f32>>> {
    let config = E5Config::default();
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    if !INITIALIZED.load(Ordering::Relaxed) {
        // Ensure models exist before initialization
        config.ensure_models_exist().await?;
        init_e5_model(&config)?;
        init_e5_tokenizer(&config)?;
        INITIALIZED.store(true, Ordering::Relaxed);
    }

    match get_article_embedding(text, &config).await {
        Ok(embedding) => {
            // Basic validation
            if embedding.len() != config.dimensions {
                error!(
                    "Unexpected embedding dimensions: got {}, expected {}",
                    embedding.len(),
                    config.dimensions
                );
                return Ok(None);
            }

            // Check if embedding contains valid floats
            if embedding.iter().any(|x| x.is_nan() || x.is_infinite()) {
                error!("Embedding contains invalid values (NaN or infinite)");
                return Ok(None);
            }

            info!(
                "Generated valid embedding with {} dimensions",
                embedding.len()
            );
            Ok(Some(embedding))
        }
        Err(e) => {
            error!("Failed to generate embedding: {:?}", e);
            Ok(None)
        }
    }
}
