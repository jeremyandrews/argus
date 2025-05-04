use anyhow::Result;
use candle_core::{DType, Device};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel, Config as BertConfig, HiddenAct, PositionEmbeddingType,
};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tokio::fs;
use tracing::{error, info};

use crate::vector::{MODEL, TARGET_VECTOR, TOKENIZER};

/// Configuration struct for the E5 embedding model
pub struct E5Config {
    pub model_path: String,
    pub tokenizer_path: String,
    pub dimensions: usize,
    pub max_length: usize,
    pub _similarity_threshold: f32,
    pub device: Device,
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
    pub async fn ensure_models_exist(&self) -> Result<()> {
        // Create models directory if it doesn't exist
        if !Path::new("models").exists() {
            fs::create_dir("models").await?;
        }

        // Check and download model file if needed
        if !Path::new(&self.model_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 model from {}", crate::vector::MODEL_URL);
            let response = reqwest::get(crate::vector::MODEL_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.model_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 model to {}", self.model_path);
        }

        // Check and download tokenizer file if needed
        if !Path::new(&self.tokenizer_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 tokenizer from {}", crate::vector::TOKENIZER_URL);
            let response = reqwest::get(crate::vector::TOKENIZER_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.tokenizer_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 tokenizer to {}", self.tokenizer_path);
        }

        Ok(())
    }
}

/// Initialize the E5 model from config
pub fn init_e5_model(config: &E5Config) -> Result<()> {
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

/// Initialize the E5 tokenizer from config
pub fn init_e5_tokenizer(config: &E5Config) -> Result<()> {
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
