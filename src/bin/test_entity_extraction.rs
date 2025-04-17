use argus::entity::extraction::extract_entities;
use argus::{LLMClient, LLMParams, WorkerDetail};
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use serde_json::to_string_pretty;
use std::env;
use tokio::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

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
    let model = env::var("ENTITY_MODEL").unwrap_or_else(|_| "llama3".to_string());
    let temperature = env::var("ENTITY_TEMPERATURE")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);

    info!("Using model: {} with temperature: {}", model, temperature);

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
            // Default to Ollama
            let host_url =
                env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());

            // Parse host and port from URL
            let url_parts: Vec<&str> = host_url.split("://").collect();
            let host_port = if url_parts.len() > 1 {
                url_parts[1]
            } else {
                url_parts[0]
            };

            let host_port_parts: Vec<&str> = host_port.split(':').collect();
            let host_name = host_port_parts[0].to_string();

            let port: u16 = if host_port_parts.len() > 1 {
                host_port_parts[1].parse().unwrap_or(11434)
            } else {
                11434 // Default Ollama port
            };

            info!("Connecting to Ollama at {}:{}", host_name, port);
            LLMClient::Ollama(Ollama::new(&host_name, port))
        }
    };

    let mut llm_params = LLMParams {
        llm_client,
        model: model.clone(),
        temperature,
        require_json: None,
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
