pub mod workers; // New modular workers organization

// Re-exports for backward compatibility
pub use workers::analysis::worker_loop as analysis_worker;
pub use workers::decision::worker_loop as decision_worker;

pub mod app {
    pub mod api;
    pub mod util;
}
pub mod clustering;
pub mod db; // Now uses the directory module structure
pub mod entity;
pub mod environment;
pub mod llm;
pub mod logging;
pub mod metrics;
pub mod prompt; // Now uses the directory module structure (replacing prompts.rs)
                // Import the modular RSS structure
pub mod rss;

// Re-export RSS module functionality for backward compatibility
pub use rss::process_rss_urls;
pub use rss::rss_loop;
pub use rss::test_rss_feed;
pub mod slack;
pub mod util;
pub mod vector;

// Re-export important vector functions for easy access
pub use vector::similarity::calculate_direct_similarity;
pub use vector::storage::get_article_vector_from_qdrant;

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

/// Configuration for models that use thinking/reasoning capabilities
#[derive(Clone, Debug)]
pub struct ThinkingModelConfig {
    pub strip_thinking_tags: bool,
    pub top_p: f32,
    pub top_k: i32,
    pub min_p: f32,
}

/// Parse a model name string to detect the /no_think suffix
///
/// Returns a tuple containing:
/// - The model name without the /no_think suffix
/// - A boolean indicating if no_think mode is enabled
///
/// # Examples
/// ```
/// use argus::parse_model_name;
/// let (model, no_think) = parse_model_name("qwen3:30b-a3b-fp16/no_think");
/// assert_eq!(model, "qwen3:30b-a3b-fp16");
/// assert_eq!(no_think, true);
/// ```
pub fn parse_model_name(model_string: &str) -> (String, bool) {
    if model_string.ends_with("/no_think") {
        // Remove the /no_think suffix and return true for no_think mode
        (model_string.trim_end_matches("/no_think").to_string(), true)
    } else {
        (model_string.to_string(), false)
    }
}

#[derive(Clone)]
pub struct LLMParams {
    pub llm_client: LLMClient,
    pub model: String,
    pub temperature: f32,
    pub require_json: Option<bool>, // Kept for backward compatibility
    pub json_format: Option<JsonSchemaType>, // New field for specifying JSON schema type
    pub thinking_config: Option<ThinkingModelConfig>, // Configuration for thinking models
    pub no_think: bool,             // Flag to indicate /no_think mode
}

// New: Struct to hold fallback configuration for Analysis Workers
#[derive(Clone, Debug)]
pub struct FallbackConfig {
    pub llm_client: LLMClient,
    pub model: String,
    pub no_think: bool, // Flag to indicate /no_think mode
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
/// The model can include the /no_think suffix to disable thinking mode.
///
/// # Returns
/// Vec of tuples containing (host, port, model, no_think)
pub fn process_ollama_configs(configs: &str) -> Vec<(String, u16, String, bool)> {
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

        // Parse model name and detect no_think mode
        let (model, no_think) = parse_model_name(parts[2]);

        results.push((host, port, model, no_think));
    }

    results
}

/// Parses Analysis Ollama configurations from a string, including fallback configurations.
///
/// The expected format is: `host|port|model||fallback_host|fallback_port|fallback_model;...`
/// Where the fallback part after `||` is optional.
/// The model can include the /no_think suffix to disable thinking mode.
///
/// # Returns
/// Vec of tuples containing (host, port, model, no_think, Option<(fallback_host, fallback_port, fallback_model, fallback_no_think)>)
pub fn process_analysis_ollama_configs(
    configs: &str,
) -> Vec<(
    String,
    u16,
    String,
    bool,
    Option<(String, u16, String, bool)>,
)> {
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

        // Parse model name and detect no_think mode
        let (main_model, main_no_think) = parse_model_name(main_parts[2]);

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

                // Parse fallback model name and detect no_think mode
                let (fallback_model, fallback_no_think) = parse_model_name(fallback_parts[2]);

                Some((
                    fallback_host,
                    fallback_port,
                    fallback_model,
                    fallback_no_think,
                ))
            }
        } else {
            None
        };

        results.push((main_host, main_port, main_model, main_no_think, fallback));
    }

    results
}
