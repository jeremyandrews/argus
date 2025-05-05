use crate::db::core::Database;
use crate::{LLMClient, LLMParams};
use std::collections::BTreeMap;

/// Parameters required for processing an item, including topics, database, and Slack channel information.
pub struct ProcessItemParams<'a> {
    pub topics: &'a [String],
    pub llm_client: &'a LLMClient,
    pub model: &'a str,
    pub temperature: f32,
    pub db: &'a Database,
    pub slack_token: &'a str,
    pub slack_channel: &'a str,
    pub places:
        BTreeMap<std::string::String, BTreeMap<std::string::String, Vec<std::string::String>>>,
}

/// Extracts LLM parameters from ProcessItemParams
pub fn extract_llm_params<'a>(params: &'a ProcessItemParams<'a>) -> LLMParams {
    LLMParams {
        llm_client: params.llm_client.clone(),
        model: params.model.to_string(),
        temperature: params.temperature,
        require_json: None,
        json_format: None,
        thinking_config: None, // No thinking by default in decision worker
        no_think: false,       // No special no_think mode by default
    }
}

/// A common structure for feed items used by workers
#[derive(Default)]
pub struct FeedItem {
    pub url: String,
    pub title: Option<String>,
    pub pub_date: Option<String>,
}

/// Builds connection info string for worker details
pub fn build_connection_info(llm_client: &LLMClient, worker_id: i16, env_var_name: &str) -> String {
    match llm_client {
        LLMClient::Ollama(_) => {
            // Since Ollama doesn't expose host/port directly, extract from env var
            match &std::env::var(env_var_name) {
                Ok(configs) => {
                    // Find the config for this worker ID
                    let all_configs: Vec<&str> = configs.split(';').collect();
                    if (worker_id as usize) < all_configs.len() {
                        let parts: Vec<&str> = all_configs[worker_id as usize].split('|').collect();
                        if parts.len() >= 2 {
                            format!("{}:{}", parts[0], parts[1])
                        } else {
                            format!("ollama-{}", worker_id)
                        }
                    } else {
                        format!("ollama-{}", worker_id)
                    }
                }
                Err(_) => format!("ollama-{}", worker_id),
            }
        }
        LLMClient::OpenAI(_) => "OpenAI API".to_string(),
    }
}

/// Converts sources_quality and argument_quality (values 1-3) into a combined quality score
/// where 1 = -1, 2 = 1, 3 = 2 points.
///
/// # Arguments
/// * `sources_quality` - Rating of sources quality from 1-3
/// * `argument_quality` - Rating of argument quality from 1-3
///
/// # Returns
/// * `i8` - Combined quality score ranging from -2 to 4
pub fn calculate_quality_score(sources_quality: u8, argument_quality: u8) -> i8 {
    // Transform values: 1 -> -1, 2 -> 1, 3 -> 2
    let sources_score = match sources_quality {
        1 => -1,
        2 => 1,
        3 => 2,
        _ => 0, // Default for invalid values
    };

    let argument_score = match argument_quality {
        1 => -1,
        2 => 1,
        3 => 2,
        _ => 0, // Default for invalid values
    };

    // Combine scores
    sources_score + argument_score
}
