use futures::future::join_all;
use ollama_rs::Ollama;
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use tokio::task;
use tracing::{debug, error, info, warn};

const TARGET_WEB_REQUEST: &str = "web_request";
const TARGET_LLM_REQUEST: &str = "llm_request";
const TARGET_DB: &str = "db";

const OLLAMA_HOST_ENV: &str = "OLLAMA_HOST";
const OLLAMA_PORT_ENV: &str = "OLLAMA_PORT";
const OLLAMA_MODEL_ENV: &str = "OLLAMA_MODEL";
const SLACK_TOKEN_ENV: &str = "SLACK_TOKEN";
const SLACK_CHANNEL_ENV: &str = "SLACK_CHANNEL";
const LLM_TEMPERATURE_ENV: &str = "LLM_TEMPERATURE";
const PLACES_JSON_PATH_ENV: &str = "PLACES_JSON_PATH";
const WORKER_COUNT_ENV: &str = "WORKER_COUNT";

mod db;
mod environment;
mod llm;
mod logging;
mod rss;
mod slack;
mod util;
mod worker;

use environment::get_env_var_as_vec;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::configure_logging();

    let urls = get_env_var_as_vec("URLS", ';');
    let ollama_host = env::var(OLLAMA_HOST_ENV).unwrap_or_else(|_| "localhost".to_string());
    let ollama_port = env::var(OLLAMA_PORT_ENV)
        .unwrap_or_else(|_| "11434".to_string())
        .parse()
        .unwrap_or_else(|_| {
            error!("Invalid OLLAMA_PORT; defaulting to 11434");
            11434
        });

    info!(target: TARGET_LLM_REQUEST, "Connecting to Ollama at {}:{}", ollama_host, ollama_port);
    let ollama = Ollama::new(ollama_host, ollama_port);
    let model = env::var(OLLAMA_MODEL_ENV).unwrap_or_else(|_| "llama2".to_string());
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

    // Configure worker count
    let worker_count: usize = env::var(WORKER_COUNT_ENV)
        .unwrap_or_else(|_| "1".to_string())
        .parse()
        .unwrap_or_else(|_| {
            warn!("Invalid WORKER_COUNT; defaulting to 1");
            1
        });

    let mut worker_handles = Vec::new();
    for worker_id in 0..worker_count {
        let ollama_worker = ollama.clone();
        let model_worker = model.clone();
        let topics_worker = topics.clone();
        let slack_token_worker = slack_token.clone();
        let slack_channel_worker = slack_channel.clone();
        let places_worker = places.clone();

        let worker_handle = task::spawn(async move {
            let mut non_affected_people = BTreeSet::new();
            let mut non_affected_places = BTreeSet::new();

            info!(target: TARGET_LLM_REQUEST, "Worker {}: Starting worker loop.", worker_id);
            // Assuming worker::worker_loop is infallible and only returns ()
            worker::worker_loop(
                &topics_worker,
                &ollama_worker,
                &model_worker,
                temperature,
                &slack_token_worker,
                &slack_channel_worker,
                places_worker,
                &mut non_affected_people,
                &mut non_affected_places,
            )
            .await;

            info!(target: TARGET_LLM_REQUEST, "Worker {}: Completed worker loop.", worker_id);
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
            error!(target: TARGET_LLM_REQUEST, "Worker {} task failed: {}", i, e);
        }
    }

    Ok(())
}
