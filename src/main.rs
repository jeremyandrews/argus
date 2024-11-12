use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use futures::future::join_all;
use ollama_rs::Ollama;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use tokio::task;
use tracing::{debug, error, info, warn};

const TARGET_WEB_REQUEST: &str = "web_request";
const TARGET_LLM_REQUEST: &str = "llm_request";
const TARGET_DB: &str = "db_query";

const DECISION_OLLAMA_CONFIGS_ENV: &str = "DECISION_OLLAMA_CONFIGS";
const ANALYSIS_OLLAMA_CONFIGS_ENV: &str = "ANALYSIS_OLLAMA_CONFIGS";
const DECISION_OPENAI_CONFIGS_ENV: &str = "DECISION_OPENAI_CONFIGS";
const ANALYSIS_OPENAI_CONFIGS_ENV: &str = "ANALYSIS_OPENAI_CONFIGS";
const SLACK_TOKEN_ENV: &str = "SLACK_TOKEN";
const SLACK_CHANNEL_ENV: &str = "SLACK_CHANNEL";
const LLM_TEMPERATURE_ENV: &str = "LLM_TEMPERATURE";
const PLACES_JSON_PATH_ENV: &str = "PLACES_JSON_PATH";

use argus::analysis_worker;
use argus::decision_worker;
use argus::environment;
use argus::logging;
use argus::rss;
use argus::LLMClient;

use environment::get_env_var_as_vec;

#[tokio::main]
async fn main() -> Result<()> {
    logging::configure_logging();

    // Read the DECISION and ANALYSIS environment variables
    let decision_ollama_configs = env::var(DECISION_OLLAMA_CONFIGS_ENV).unwrap_or_default();
    let analysis_ollama_configs = env::var(ANALYSIS_OLLAMA_CONFIGS_ENV).unwrap_or_default();
    let decision_openai_configs = env::var(DECISION_OPENAI_CONFIGS_ENV).unwrap_or_default();
    let analysis_openai_configs = env::var(ANALYSIS_OPENAI_CONFIGS_ENV).unwrap_or_default();

    let mut decision_workers = Vec::new();
    let mut decision_count: i16 = 0;

    let mut analysis_workers = Vec::new();
    let mut analysis_count: i16 = 0;

    // Helper function to process configurations
    fn process_ollama_configs(
        configs: &str,
        workers: &mut Vec<(i16, LLMClient, String)>,
        count: &mut i16,
    ) {
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
            info!(
                "Configuring Ollama worker {} to connect to model '{}' at {}:{}",
                *count, model, host, port
            );
            workers.push((*count, LLMClient::Ollama(Ollama::new(host, port)), model));
            *count += 1;
        }
    }

    fn process_openai_configs(
        configs: &str,
        workers: &mut Vec<(i16, LLMClient, String)>,
        count: &mut i16,
    ) {
        for config in configs.split(';').filter(|c| !c.is_empty()) {
            let parts: Vec<&str> = config.split('|').collect();
            if parts.len() != 2 {
                error!("Invalid OpenAI configuration format: {}", config);
                continue;
            }
            let api_key = parts[0].to_string();
            let model = parts[1].to_string();
            let config = OpenAIConfig::new().with_api_key(&api_key);
            let client = OpenAIClient::with_config(config);
            info!(
                "Configuring OpenAI worker {} to connect to model '{}'",
                *count, model
            );
            workers.push((*count, LLMClient::OpenAI(client), model));
            *count += 1;
        }
    }

    // Process DECISION configurations
    process_ollama_configs(
        &decision_ollama_configs,
        &mut decision_workers,
        &mut decision_count,
    );
    process_openai_configs(
        &decision_openai_configs,
        &mut decision_workers,
        &mut decision_count,
    );

    // Log DECISION workers
    info!(
        "Total decision workers configured: {}",
        decision_workers.len()
    );

    // Load ANALYSIS configurations (for now, just logging)
    process_ollama_configs(
        &analysis_ollama_configs,
        &mut analysis_workers,
        &mut analysis_count,
    );
    process_openai_configs(
        &analysis_openai_configs,
        &mut analysis_workers,
        &mut analysis_count,
    );

    // Log ANALYSIS workers
    info!(
        "Total analysis workers configured: {}",
        analysis_workers.len()
    );

    // Determine number of decision workers to launch
    let decision_worker_count = decision_ollama_configs
        .split(';')
        .filter(|c| !c.is_empty())
        .count()
        + decision_openai_configs
            .split(';')
            .filter(|c| !c.is_empty())
            .count();

    let urls = get_env_var_as_vec("URLS", ';');
    let topics = get_env_var_as_vec("TOPICS", ';');
    let slack_token = env::var(SLACK_TOKEN_ENV).expect("SLACK_TOKEN environment variable required");
    let slack_channel =
        env::var(SLACK_CHANNEL_ENV).expect("SLACK_CHANNEL environment variable required");
    let temperature = env::var(LLM_TEMPERATURE_ENV)
        .unwrap_or_else(|_| "0.0".to_string())
        .parse()
        .unwrap_or_else(|_| {
            warn!("Invalid LLM_TEMPERATURE; defaulting to 0.0");
            0.0
        });

    // Load JSON data if PLACES_JSON_PATH environment variable is set
    let places = if let Ok(json_path) = env::var(PLACES_JSON_PATH_ENV) {
        if Path::new(&json_path).exists() {
            let json_data = fs::read_to_string(&json_path)?;
            let places: Value = serde_json::from_str(&json_data)?;
            info!(target: TARGET_DB, "Loaded places data from {}: {:?}", json_path, places);
            Some(places)
        } else {
            warn!(target: TARGET_DB, "Specified PLACES_JSON_PATH does not exist: {}", json_path);
            None
        }
    } else {
        debug!(target: TARGET_DB, "PLACES_JSON_PATH environment variable not set.");
        None
    };

    // Spawn a thread to parse URLs from RSS feeds.
    let rss_handle = task::spawn(async move {
        info!(target: TARGET_WEB_REQUEST, "Starting RSS feed parsing.");
        match rss::rss_loop(urls.clone()).await {
            Ok(_) => info!(target: TARGET_WEB_REQUEST, "RSS feed parsing completed successfully."),
            Err(e) => error!(target: TARGET_WEB_REQUEST, "RSS feed parsing failed: {}", e),
        }
    });

    // Launch DECISION workers
    let mut decision_handles = Vec::new();
    for (decision_id, llm_client, decision_model) in
        decision_workers.into_iter().take(decision_worker_count)
    {
        let decision_worker_topics = topics.clone();
        let decision_worker_slack_token = slack_token.clone();
        let decision_worker_slack_channel = slack_channel.clone();
        let decision_worker_places = places.clone();
        let decision_worker_handle = task::spawn(async move {
            info!(
                target: TARGET_LLM_REQUEST,
                "Decision worker {}: starting with model '{}'",
                decision_id, decision_model
            );
            decision_worker::decision_loop(
                decision_id,
                &decision_worker_topics,
                &llm_client,
                &decision_model,
                temperature,
                &decision_worker_slack_token,
                &decision_worker_slack_channel,
                decision_worker_places,
            )
            .await;
            info!(
                target: TARGET_LLM_REQUEST,
                "Decision worker {}: completed decision_loop for model '{}'",
                decision_id, decision_model
            );
        });
        decision_handles.push(decision_worker_handle);
    }

    // Launch ANALYSIS workers
    let mut analysis_handles = Vec::new();
    for (analysis_id, llm_client, analysis_model) in analysis_workers.into_iter() {
        let analysis_worker_slack_token = slack_token.clone();
        let analysis_worker_slack_channel = slack_channel.clone();
        let analysis_worker_handle = task::spawn(async move {
            info!(
                target: TARGET_LLM_REQUEST,
                "Analysis worker {}: starting with model '{}'",
                analysis_id, analysis_model
            );
            analysis_worker::analysis_loop(
                analysis_id,
                &llm_client,
                &analysis_model,
                &analysis_worker_slack_token,
                &analysis_worker_slack_channel,
                // @TODO: allow different temperatures for Decision and Analysis workers.
                temperature,
            )
            .await;
            info!(
                target: TARGET_LLM_REQUEST,
                "Analysis worker {}: completed analysis_loop for model '{}'",
                analysis_id, analysis_model
            );
        });
        analysis_handles.push(analysis_worker_handle);
    }

    // Await task completions
    if let Err(e) = rss_handle.await {
        error!(target: TARGET_WEB_REQUEST, "RSS task encountered an error: {}", e);
    }

    // Await decision worker tasks
    let results = join_all(decision_handles).await;
    for (i, result) in results.into_iter().enumerate() {
        if let Err(e) = result {
            error!(target: TARGET_LLM_REQUEST, "Decision worker {}: task failed with error: {}", i, e);
        }
    }

    // Await analysis worker tasks
    let analysis_results = join_all(analysis_handles).await;
    for (i, result) in analysis_results.into_iter().enumerate() {
        if let Err(e) = result {
            error!(target: TARGET_LLM_REQUEST, "Analysis worker {}: task failed with error: {}", i, e);
        }
    }

    Ok(())
}
