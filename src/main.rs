use ollama_rs::Ollama;
use std::env;
use tracing::info;

const TARGET_WEB_REQUEST: &str = "web_request";
const TARGET_LLM_REQUEST: &str = "llm_request";

mod db;
use db::Database;

mod environment;
mod llm;
mod logging;
mod slack;
mod web;

use environment::get_env_var_as_vec;
use web::{process_urls, ProcessItemParams};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logging::configure_logging();

    let urls = get_env_var_as_vec("URLS", ';');
    let ollama_host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost".to_string());
    let ollama_port = env::var("OLLAMA_PORT")
        .unwrap_or_else(|_| "11434".to_string())
        .parse()
        .unwrap_or(11434);

    info!(target: TARGET_LLM_REQUEST, "Connecting to Ollama at {}:{}", ollama_host, ollama_port);
    let ollama = Ollama::new(ollama_host, ollama_port);
    let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());
    let topics = get_env_var_as_vec("TOPICS", ';');
    let slack_token = env::var("SLACK_TOKEN").expect("SLACK_TOKEN environment variable required");
    let slack_channel =
        env::var("SLACK_CHANNEL").expect("SLACK_CHANNEL environment variable required");
    let db_path = env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());
    let db = Database::new(&db_path).expect("Failed to initialize database");
    let temperature = env::var("LLM_TEMPERATURE")
        .unwrap_or_else(|_| "0.0".to_string())
        .parse()
        .unwrap_or(0.0);

    let params = ProcessItemParams {
        topics: &topics,
        ollama: &ollama,
        model: &model,
        temperature,
        db: &db,
        slack_token: &slack_token,
        slack_channel: &slack_channel,
    };

    process_urls(urls, &params).await?;

    Ok(())
}
