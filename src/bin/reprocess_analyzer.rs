use anyhow::Result;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read;
use tracing::{info, warn};
use tracing_subscriber;

use argus::llm;
use argus::prompts;
use argus::{LLMClient, LLMParams};

const TEST_DATA_DIR: &str = "test_data";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    info!("Starting JSON re-analyzer...");

    // Load JSON files from the `test_data` directory
    let json_files = fs::read_dir(TEST_DATA_DIR)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if json_files.is_empty() {
        warn!("No JSON files found in the test_data directory.");
        return Ok(());
    }

    for file_entry in json_files {
        let file_path = file_entry.path();
        info!("Processing file: {}", file_path.display());

        let mut file = File::open(&file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let json_data: Value = serde_json::from_str(&content)?;

        // Extract relevant fields from JSON
        let body = json_data["body"].as_str().unwrap_or("No Content");
        let stored_relevance = json_data["relevance"]
            .as_object()
            .cloned()
            .unwrap_or_default();

        let mut new_relevance = serde_json::Map::new();
        for (topic_key, topic_name) in get_topics() {
            let prompt = crate::prompts::is_this_about(body, topic_name);

            let llm_base_url = std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
            let llm_host_with_scheme = format!(
                "{}://{}",
                url::Url::parse(&llm_base_url).unwrap().scheme(),
                url::Url::parse(&llm_base_url).unwrap().host_str().unwrap()
            );
            let llm_port = url::Url::parse(&llm_base_url)
                .unwrap()
                .port()
                .unwrap_or(11434);
            let llm_model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "llama3.1".to_string());

            let llm_params = crate::LLMParams {
                llm_client: &LLMClient::Ollama(ollama_rs::Ollama::new(
                    llm_host_with_scheme.clone(),
                    llm_port,
                )),
                model: &llm_model,
                temperature: 0.0,
            };

            if let Some(response) = crate::llm::generate_llm_response(&prompt, &llm_params).await {
                new_relevance.insert(
                    topic_key.to_string(),
                    Value::String(response.trim().to_string()),
                );
            } else {
                new_relevance.insert(topic_key.to_string(), Value::String("unknown".to_string()));
            }
        }

        // Compare new relevance with the stored one
        for (topic_key, stored_value) in stored_relevance.iter() {
            let unknown_value = Value::String("unknown".to_string());
            let new_value = new_relevance.get(topic_key).unwrap_or(&unknown_value);

            if new_value != stored_value {
                info!(
                    "Mismatch for topic '{}': Stored = {}, New = {}",
                    topic_key, stored_value, new_value
                );
            }
        }

        info!("Re-analysis completed for file: {}", file_path.display());
    }

    Ok(())
}

fn get_topics() -> Vec<(&'static str, &'static str)> {
    vec![
        ("apple", "New Apple products, like new versions of iPhone, iPad and MacBooks, or newly announced products"),
        ("space", "Space and Space Exploration"),
        ("longevity", "Advancements in health practices and technologies that enhance human longevity"),
        ("llm", "significant new developments in Large Language Models, or anything about the Llama LLM"),
        ("ev", "Electric vehicles"),
        ("rust", "the Rust programming language"),
        ("bitcoin", "Bitcoins, the cryptocurrency"),
        ("drupal", "the Drupal Content Management System"),
        ("linux_vuln", "a major new vulnerability in Linux, macOS, or iOS"),
        ("global_vuln", "a global vulnerability bringing down significant infrastructure worldwide"),
        ("tuscany", "Tuscany, the famous region in Italy"),
    ]
}