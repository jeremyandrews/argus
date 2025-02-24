use anyhow::Result;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel, Config as BertConfig, HiddenAct, PositionEmbeddingType,
};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tracing::{error, info};

// Static globals
static MODEL: OnceCell<Arc<BertModel>> = OnceCell::new();
static TOKENIZER: OnceCell<Arc<Tokenizer>> = OnceCell::new();

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
            model_path: "models/e5-large.safetensors".to_string(),
            tokenizer_path: "models/e5-tokenizer.json".to_string(),
            dimensions: 1024,
            max_length: 512,
            _similarity_threshold: 0.85,
            device: Device::Cpu,
        }
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
        hidden_dropout_prob: 0.1,
        type_vocab_size: 2,
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
        init_e5_model(&config)?;
        init_e5_tokenizer(&config)?;
        INITIALIZED.store(true, Ordering::Relaxed);
    }

    match get_article_embedding(text, &config).await {
        Ok(embedding) => {
            info!("Generated embedding with {} dimensions", embedding.len());
            Ok(Some(embedding))
        }
        Err(e) => {
            error!("Failed to generate embedding: {:?}", e);
            Ok(None)
        }
    }
}
