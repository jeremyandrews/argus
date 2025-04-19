//! # Entity Extraction Test Utility
//!
//! This utility tests the entity extraction functionality on a sample article.
//!
//! ## Usage
//!
//! ```
//! cargo run --bin test_entity_extraction
//! ```
//!
//! ## Configuration
//!
//! The utility uses the following environment variables:
//! - `ANALYSIS_OLLAMA_CONFIGS`: Ollama configuration in format "host|port|model;..." for analysis workers
//! - `ENTITY_MODEL`: Override specific model to use (optional)
//! - `ENTITY_TEMPERATURE`: Temperature setting for entity extraction (default: 0.0)
//! - `ENTITY_LLM_TYPE`: Type of LLM to use ("ollama" or "openai", default: "ollama")
//! - `OPENAI_API_KEY`: OpenAI API key (required if ENTITY_LLM_TYPE is "openai")
//!
//! ## Purpose
//!
//! This utility is used to verify that entity extraction is working correctly by processing
//! a sample article and displaying the extracted entities. It helps diagnose issues with the
//! entity extraction pipeline and confirms that the LLM is properly configured to return
//! structured entity data in JSON format.

use argus::entity::extraction::extract_entities;
use argus::{LLMClient, LLMParams, WorkerDetail};
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use serde_json::to_string_pretty;
use std::env;
use tokio::time::Instant;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

/// Process analysis worker configuration and return client and model
fn process_analysis_worker_config(configs: &str) -> Option<(LLMClient, String)> {
    let analysis_configs = argus::process_analysis_ollama_configs(configs);

    if analysis_configs.is_empty() {
        error!("No valid configurations found in ANALYSIS_OLLAMA_CONFIGS");
        return None;
    }

    let (host, port, model, _) = &analysis_configs[0];

    info!("Using analysis worker configuration with model: {}", model);
    info!("Connecting to Ollama at {}:{}", host, port);

    Some((
        LLMClient::Ollama(Ollama::new(host.clone(), *port)),
        model.clone(),
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging - use debug level to see more detailed information
    let subscriber = FmtSubscriber::builder()
        .with_max_level(match std::env::var("RUST_LOG") {
            Ok(level) => match level.to_lowercase().as_str() {
                "trace" => Level::TRACE,
                "debug" => Level::DEBUG,
                "info" => Level::INFO,
                "warn" => Level::WARN,
                "error" => Level::ERROR,
                _ => Level::DEBUG, // Default to DEBUG if RUST_LOG is set but invalid
            },
            Err(_) => Level::DEBUG, // Default to DEBUG if RUST_LOG is not set
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Entity extraction test started with debug logging enabled");

    // Test article text
    let article_text = r#"
    Apple announced its new iPad Pro today, featuring the M4 chip. 
    CEO Tim Cook unveiled the device at a special event in Cupertino, California.
    The new tablet will be available starting next week at Apple Stores worldwide,
    with prices beginning at $999. Google and Microsoft were quick to respond,
    with Microsoft announcing updates to their Surface lineup.
    "#;

    // Set publication date (optional)
    let pub_date = Some("2025-04-17");

    // Set up LLM parameters
    let mut model = env::var("ENTITY_MODEL").unwrap_or_else(|_| "llama3".to_string());
    let temperature = env::var("ENTITY_TEMPERATURE")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);

    // Configure LLM client
    let llm_client = match env::var("ENTITY_LLM_TYPE")
        .unwrap_or_else(|_| "ollama".to_string())
        .as_str()
    {
        "openai" => {
            let api_key = env::var("OPENAI_API_KEY")
                .expect("OPENAI_API_KEY environment variable must be set");
            let config = OpenAIConfig::new().with_api_key(api_key);
            LLMClient::OpenAI(OpenAIClient::with_config(config))
        }
        _ => {
            // Get the analysis worker configs
            let analysis_ollama_configs = env::var("ANALYSIS_OLLAMA_CONFIGS")
                .expect("ANALYSIS_OLLAMA_CONFIGS environment variable must be set");

            let (client, config_model) = process_analysis_worker_config(&analysis_ollama_configs)
                .expect("Failed to process analysis worker configuration");

            // Update model if we're using default and config provides one
            if model == "llama3" {
                info!("Using model {} from analysis configuration", config_model);
                model = config_model;
            }

            client
        }
    };

    info!("Using model: {} with temperature: {}", model, temperature);

    let mut llm_params = LLMParams {
        llm_client,
        model: model.clone(),
        temperature,
        require_json: None,
        json_format: None,
    };

    let worker_detail = WorkerDetail {
        name: "entity test".to_string(),
        id: 0,
        model: model.clone(),
        connection_info: "entity test".to_string(),
    };

    // Extract entities
    let start_time = Instant::now();
    info!("Starting entity extraction...");

    match extract_entities(article_text, pub_date, &mut llm_params, &worker_detail).await {
        Ok(extracted_entities) => {
            let elapsed = start_time.elapsed();
            info!(
                "Successfully extracted {} entities in {:?}",
                extracted_entities.entities.len(),
                elapsed
            );

            // Print the event date if present
            if let Some(event_date) = &extracted_entities.event_date {
                info!("Event date: {}", event_date);
            } else {
                info!("No event date extracted");
            }

            // Print extracted entities
            info!("Extracted entities:");
            for (i, entity) in extracted_entities.entities.iter().enumerate() {
                info!(
                    "{}: {} ({}), Type: {:?}, Importance: {:?}",
                    i + 1,
                    entity.name,
                    entity.normalized_name,
                    entity.entity_type,
                    entity.importance
                );

                // Print metadata if available
                if let Some(metadata) = &entity.metadata {
                    info!("   Metadata: {}", to_string_pretty(metadata)?);
                }
            }

            // Print as JSON
            info!(
                "Full extraction result:\n{}",
                to_string_pretty(&extracted_entities)?
            );
        }
        Err(e) => {
            println!("Failed to extract entities: {:?}", e);
        }
    }

    Ok(())
}
