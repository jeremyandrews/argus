pub mod analysis_worker;
pub mod app {
    pub mod api;
    pub mod util;
}
pub mod db;
pub mod decision_worker;
pub mod entity;
pub mod environment;
pub mod llm;
pub mod logging;
pub mod metrics;
pub mod prompts;
pub mod rss;
pub mod slack;
pub mod util;
pub mod vector;

use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use serde::Serialize;
use std::sync::atomic::AtomicU64;
use tracing::error;

pub const TARGET_WEB_REQUEST: &str = "web_request";
pub const TARGET_LLM_REQUEST: &str = "llm_request";
pub const TARGET_DB: &str = "db_query";

pub static START_TIME: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub enum LLMClient {
    Ollama(Ollama),
    OpenAI(OpenAIClient<OpenAIConfig>),
}

/// Enum defining available JSON schema types for structured LLM responses
#[derive(Clone, Debug)]
pub enum JsonSchemaType {
    /// For entity extraction: returns entities array
    EntityExtraction,

    /// For threat location: returns impacted_regions array
    ThreatLocation,

    /// Generic JSON response without schema enforcement
    Generic,
}

#[derive(Clone)]
pub struct LLMParams {
    pub llm_client: LLMClient,
    pub model: String,
    pub temperature: f32,
    pub require_json: Option<bool>, // Kept for backward compatibility
    pub json_format: Option<JsonSchemaType>, // New field for specifying JSON schema type
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
    pub connection_info: String, // Contains host:port for Ollama or API endpoint for OpenAI
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

/// Parses Ollama configurations from a string.
///
/// The expected format is: `host|port|model;host|port|model;...`
///
/// # Returns
/// Vec of tuples containing (host, port, model)
pub fn process_ollama_configs(configs: &str) -> Vec<(String, u16, String)> {
    let mut results = Vec::new();

    for config in configs.split(';').filter(|c| !c.is_empty()) {
        let parts: Vec<&str> = config.split('|').collect();
        if parts.len() != 3 {
            error!("Invalid Ollama configuration format: {}", config);
            continue;
        }
        let host = parts[0].to_string();
        let port: u16 = parts[1].parse().unwrap_or_else(|_| {
            error!("Invalid port in configuration: {}", parts[1]);
            11434 // Default port
        });
        let model = parts[2].to_string();

        results.push((host, port, model));
    }

    results
}

/// Parses Analysis Ollama configurations from a string, including fallback configurations.
///
/// The expected format is: `host|port|model||fallback_host|fallback_port|fallback_model;...`
/// Where the fallback part after `||` is optional.
///
/// # Returns
/// Vec of tuples containing (host, port, model, Option<(fallback_host, fallback_port, fallback_model)>)
pub fn process_analysis_ollama_configs(
    configs: &str,
) -> Vec<(String, u16, String, Option<(String, u16, String)>)> {
    let mut results = Vec::new();

    for config in configs.split(';').filter(|c| !c.is_empty()) {
        // Split main and fallback configurations
        let parts: Vec<&str> = config.split("||").collect();
        if parts.is_empty() {
            error!("Invalid Analysis Ollama configuration format: {}", config);
            continue;
        }

        // Process main configuration
        let main_parts: Vec<&str> = parts[0].split('|').collect();
        if main_parts.len() != 3 {
            error!(
                "Invalid main Ollama configuration format for Analysis worker: {}",
                parts[0]
            );
            continue;
        }
        let main_host = main_parts[0].to_string();
        let main_port: u16 = main_parts[1].parse().unwrap_or_else(|_| {
            error!("Invalid port in main configuration: {}", main_parts[1]);
            11434 // Default port
        });
        let main_model = main_parts[2].to_string();

        // Process fallback configuration if present
        let fallback = if parts.len() > 1 {
            let fallback_parts: Vec<&str> = parts[1].split('|').collect();
            if fallback_parts.len() != 3 {
                error!(
                    "Invalid fallback Ollama configuration format for Analysis worker: {}",
                    parts[1]
                );
                None
            } else {
                let fallback_host = fallback_parts[0].to_string();
                let fallback_port: u16 = fallback_parts[1].parse().unwrap_or_else(|_| {
                    error!(
                        "Invalid port in fallback configuration: {}",
                        fallback_parts[1]
                    );
                    11434 // Default port
                });
                let fallback_model = fallback_parts[2].to_string();
                Some((fallback_host, fallback_port, fallback_model))
            }
        } else {
            None
        };

        results.push((main_host, main_port, main_model, fallback));
    }

    results
}
