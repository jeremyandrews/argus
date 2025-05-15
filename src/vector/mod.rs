// Vector embedding and similarity configuration
pub const TARGET_VECTOR: &str = "article-embeddings";
pub const QDRANT_URL_ENV: &str = "QDRANT_URL";
pub const MODEL_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/model.safetensors";
pub const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/tokenizer.json";

use anyhow::Result;
use candle_transformers::models::bert::BertModel;
use std::sync::{Arc, OnceLock};
use tokenizers::Tokenizer;

// Static variables for model and tokenizer
pub static MODEL: OnceLock<Arc<BertModel>> = OnceLock::new();
pub static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();

pub mod config;
pub mod embedding;
pub mod search;
pub mod similarity;
pub mod storage;
pub mod types;

// Re-export main components
pub use config::*;
pub use embedding::*;
pub use search::*;
pub use similarity::*;
pub use storage::*;
pub use types::*;

use crate::LLMClient;
use ollama_rs::Ollama;

/// Returns a reference to the model, if initialized
pub fn model() -> Result<Arc<BertModel>> {
    MODEL
        .get()
        .ok_or_else(|| anyhow::anyhow!("Model not initialized"))
        .map(Arc::clone)
}

/// Returns a reference to the tokenizer, if initialized
pub fn tokenizer() -> Result<Arc<Tokenizer>> {
    TOKENIZER
        .get()
        .ok_or_else(|| anyhow::anyhow!("Tokenizer not initialized"))
        .map(Arc::clone)
}

/// Returns the default LLM client for vector operations
pub fn get_default_llm_client() -> LLMClient {
    // Parse host and port from environment or use defaults
    let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port: u16 = std::env::var("OLLAMA_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(11434);

    // Initialize the Ollama client with the base URL
    LLMClient::Ollama(Ollama::new(host, port))
}
