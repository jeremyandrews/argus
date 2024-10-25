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

const OLLAMA_CONFIGS_ENV: &str = "OLLAMA_CONFIGS";
const SLACK_TOKEN_ENV: &str = "SLACK_TOKEN";
const SLACK_CHANNEL_ENV: &str = "SLACK_CHANNEL";
const LLM_TEMPERATURE_ENV: &str = "LLM_TEMPERATURE";
const PLACES_JSON_PATH_ENV: &str = "PLACES_JSON_PATH";

mod db;
mod environment;
mod llm;
mod logging;
mod prompts;
mod rss;
mod slack;
mod util;
mod worker;

use environment::get_env_var_as_vec;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::configure_logging();

    // Read the OLLAMA_CONFIGS environment variable
    let configs = env::var(OLLAMA_CONFIGS_ENV).unwrap_or_else(|_| {
        error!("{} is not set", OLLAMA_CONFIGS_ENV);
        String::new()
    });

    // Split the configurations
    let config_entries: Vec<&str> = configs.split(';').collect();
    let worker_count = config_entries.len();

    let mut workers = Vec::new();
    let mut count: i16 = 0;
    for config in config_entries {
        // Split each configuration into host, port, and model
        let parts: Vec<&str> = config.split('|').collect();
        if parts.len() != 3 {
            error!("Invalid configuration format: {}", config);
            continue;
        }

        let host = parts[0].to_string();
        let port: u16 = parts[1].parse().unwrap_or_else(|_| {
            error!("Invalid port in configuration: {}", parts[1]);
            11434 // Default port
        });
        let model = parts[2].to_string();

        info!(
            "Configuring worker {} to connect to model '{}' at {}:{}",
            count, model, host, port
        );

        // Store the worker configuration
        workers.push((count, host, port, model));
        count += 1;
    }

    info!("Total workers configured: {}", worker_count);

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
    let urls_clone = urls.clone();
    let rss_handle = task::spawn(async move {
        info!(target: TARGET_WEB_REQUEST, "Starting RSS feed parsing.");
        match rss::rss_loop(urls_clone).await {
            Ok(_) => info!(target: TARGET_WEB_REQUEST, "RSS feed parsing completed successfully."),
            Err(e) => error!(target: TARGET_WEB_REQUEST, "RSS feed parsing failed: {}", e),
        }
    });

    let mut worker_handles = Vec::new();
    for (worker_id, host, port, model_worker) in workers {
        let ollama_worker = Ollama::new(host.clone(), port);
        let topics_worker = topics.clone();
        let slack_token_worker = slack_token.clone();
        let slack_channel_worker = slack_channel.clone();
        let places_worker = places.clone();

        let worker_handle = task::spawn(async move {
            info!(target: TARGET_LLM_REQUEST, "Worker {}: Starting with model '{}' at {}:{}.", worker_id, model_worker, host, port);
            // Log each step in the worker loop
            worker::worker_loop(
                worker_id,
                &topics_worker,
                &ollama_worker,
                &model_worker,
                temperature,
                &slack_token_worker,
                &slack_channel_worker,
                places_worker,
            )
            .await;

            info!(target: TARGET_LLM_REQUEST, "Worker {}: Completed worker loop for model '{}'.", worker_id, model_worker);
        });

        worker_handles.push(worker_handle);
    }

    // Await the completion of the RSS and worker tasks and log any errors
    if let Err(e) = rss_handle.await {
        error!(target: TARGET_WEB_REQUEST, "RSS task encountered an error: {}", e);
    }

    let results = join_all(worker_handles).await;
    for (i, result) in results.into_iter().enumerate() {
        if let Err(e) = result {
            error!(target: TARGET_LLM_REQUEST, "Worker {}: Task failed with error: {}", i, e);
        }
    }

    Ok(())
}
