use anyhow::Result;
use candle_transformers::models::bert::BertModel;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokenizers::Tokenizer;

// Submodules
pub mod config;
pub mod embedding;
pub mod search;
pub mod similarity;
pub mod storage;
pub mod types;

// Re-export commonly used types and functions
pub use config::E5Config;
pub use embedding::get_article_vectors;
pub use search::{get_article_entities, get_similar_articles, get_similar_articles_with_entities};
pub use similarity::{calculate_direct_similarity, calculate_vector_similarity};
pub use storage::{get_article_vector_from_qdrant, store_embedding};
pub use types::{ArticleMatch, NearMissMatch};

// Constants
pub const TARGET_VECTOR: &str = "vector";
pub const MODEL_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/model.safetensors";
pub const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/tokenizer.json";
pub const QDRANT_URL_ENV: &str = "QDRANT_URL";

// Global statics for model and tokenizer
pub static MODEL: OnceCell<Arc<BertModel>> = OnceCell::new();
pub static TOKENIZER: OnceCell<Arc<Tokenizer>> = OnceCell::new();

/// Get a reference to the initialized E5 model
pub fn model() -> Result<Arc<BertModel>> {
    MODEL
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("E5 model not initialized"))
}

/// Get a reference to the initialized E5 tokenizer
pub fn tokenizer() -> Result<Arc<Tokenizer>> {
    TOKENIZER
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("E5 tokenizer not initialized"))
}
