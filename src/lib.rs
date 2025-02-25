pub mod analysis_worker;
pub mod app {
    pub mod api;
    pub mod util;
}
pub mod db;
pub mod decision_worker;
pub mod environment;
pub mod llm;
pub mod logging;
pub mod metrics;
pub mod prompts;
pub mod rss;
pub mod slack;
pub mod util;

use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use serde::Serialize;
use std::sync::atomic::AtomicU64;

pub const TARGET_WEB_REQUEST: &str = "web_request";
pub const TARGET_LLM_REQUEST: &str = "llm_request";
pub const TARGET_DB: &str = "db_query";

pub static START_TIME: AtomicU64 = AtomicU64::new(0);

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
    pub require_json: Option<bool>, // Optional field to specify JSON requirement
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

#[derive(Serialize)]
pub struct SubscriptionInfo {
    pub topic: String,
    pub priority: String,
}

#[derive(Serialize)]
pub struct SubscriptionsResponse {
    pub subscriptions: Vec<SubscriptionInfo>,
}
