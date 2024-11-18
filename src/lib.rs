pub mod analysis_worker;
pub mod db;
pub mod decision_worker;
pub mod environment;
pub mod llm;
pub mod logging;
pub mod prompts;
pub mod rss;
pub mod slack;
pub mod util;

use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;

pub const TARGET_WEB_REQUEST: &str = "web_request";
pub const TARGET_LLM_REQUEST: &str = "llm_request";
pub const TARGET_DB: &str = "db_query";

#[derive(Clone, Debug)]
pub enum LLMClient {
    Ollama(Ollama),
    OpenAI(OpenAIClient<OpenAIConfig>),
}

#[derive(Clone)]
pub struct LLMParams {
    pub llm_client: LLMClient,
    pub model: String,
    pub temperature: f32,
}

// New: Struct to hold fallback configuration for Analysis Workers
#[derive(Clone, Debug)]
pub struct FallbackConfig {
    pub llm_client: LLMClient,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct WorkerDetail {
    pub name: String,
    pub id: i16,
    pub model: String,
    // @TODO: Ollama or OpenAI
    //pub client: String,
}
