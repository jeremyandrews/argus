//! # Entity Processing Utility
//!
//! This utility processes existing articles in the database to extract and store named entities.
//!
//! ## Usage
//!
//! ```
//! # Process 100 articles starting from ID 0 (default)
//! cargo run --bin process_entities
//!
//! # Process a specific number of articles
//! cargo run --bin process_entities 200
//!
//! # Process articles starting from a specific ID
//! cargo run --bin process_entities 100 500
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
//! This utility is used to populate entity data for articles that were processed before
//! entity extraction was implemented or when entity extraction was not working correctly.
//! It extracts named entities (people, organizations, locations, etc.) from articles and
//! stores them in the database for entity-based article matching and clustering.

use argus::db::Database;
use argus::entity::extraction::extract_entities;
use argus::{
    JsonLLMParams, JsonSchemaType, LLMClient, LLMParamsBase, WorkerDetail, DEFAULT_OLLAMA_HOST,
    DEFAULT_OLLAMA_MODEL, DEFAULT_OLLAMA_PORT,
};
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use sqlx::Row;
use std::env;
use tokio::time::Instant;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Process analysis worker configuration and return client and model
fn process_analysis_worker_config(configs: &str) -> Option<(LLMClient, String, bool)> {
    let analysis_configs = argus::process_analysis_ollama_configs(configs);

    if analysis_configs.is_empty() {
        // Fall back to default configuration if none found
        info!("No valid configurations found. Using default configuration.");
        info!(
            "Default host: {}, port: {}, model: {}",
            DEFAULT_OLLAMA_HOST, DEFAULT_OLLAMA_PORT, DEFAULT_OLLAMA_MODEL
        );

        return Some((
            LLMClient::Ollama(Ollama::new(
                DEFAULT_OLLAMA_HOST.to_string(),
                DEFAULT_OLLAMA_PORT,
            )),
            DEFAULT_OLLAMA_MODEL.to_string(),
            false, // Default to no-think mode disabled
        ));
    }

    let (host, port, model, no_think, _fallback) = &analysis_configs[0];

    info!("Using analysis worker configuration with model: {}", model);
    info!("Connecting to Ollama at {}:{}", host, port);
    if *no_think {
        info!("No-think mode is enabled for this model");
    }

    Some((
        LLMClient::Ollama(Ollama::new(host.clone(), *port)),
        model.clone(),
        *no_think,
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let limit = args
        .get(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(100);
    let start_from = args.get(2).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);

    info!(
        "Processing entities for up to {} articles starting from ID {}",
        limit, start_from
    );

    // Get database connection
    let db = Database::instance().await;

    // Set up LLM parameters
    let mut model = env::var("ENTITY_MODEL").unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());
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
            // Get the analysis worker configs or use empty string to trigger defaults
            let analysis_ollama_configs = env::var("ANALYSIS_OLLAMA_CONFIGS").unwrap_or_default();

            let (client, config_model, no_think_enabled) =
                process_analysis_worker_config(&analysis_ollama_configs)
                    .expect("Failed to process analysis worker configuration");

            // Update model if we're using default and config provides one
            if model == DEFAULT_OLLAMA_MODEL {
                info!("Using model {} from analysis configuration", config_model);
                model = config_model;
            }

            // We'll use the no_think setting when we create LLMParams
            if no_think_enabled {
                info!("No-think mode will be enabled for this processing run");
            }

            client
        }
    };

    // Get the no_think setting from environment or use the default from config
    let use_no_think = env::var("ENTITY_NO_THINK")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or_else(|_| {
            match llm_client {
                LLMClient::Ollama(_) => {
                    // Check if model is a Qwen model
                    if model.to_lowercase().contains("qwen") {
                        // For Qwen models, use the no_think value from the config
                        let configs = env::var("ANALYSIS_OLLAMA_CONFIGS").unwrap_or_default();
                        process_analysis_worker_config(&configs)
                            .map(|(_, _, no_think)| no_think)
                            .unwrap_or(false)
                    } else {
                        false
                    }
                }
                _ => false,
            }
        });

    let mut llm_params = JsonLLMParams {
        base: LLMParamsBase {
            llm_client: llm_client.clone(),
            model: model.clone(),
            temperature,
            thinking_config: None,  // No thinking mode for entity extraction
            no_think: use_no_think, // Apply no_think mode if enabled
        },
        schema_type: JsonSchemaType::EntityExtraction,
    };

    let worker_detail = WorkerDetail {
        name: "entity processor".to_string(),
        id: 0,
        model: model.clone(),
        connection_info: "entity processor".to_string(),
    };

    // Query to get articles with content but no entities
    let query = format!(
        "SELECT a.id, a.analysis 
         FROM articles a 
         LEFT JOIN (
             SELECT DISTINCT article_id FROM article_entities
         ) ae ON a.id = ae.article_id 
         WHERE ae.article_id IS NULL 
         AND a.analysis IS NOT NULL 
         AND a.id >= {}
         ORDER BY a.id ASC 
         LIMIT {}",
        start_from, limit
    );

    // Get the connection pool
    let pool = db.pool();

    // Execute the query
    let articles = sqlx::query(&query).fetch_all(pool).await?;

    let total_articles = articles.len();
    info!("Found {} articles to process", total_articles);

    if total_articles == 0 {
        info!("No articles to process. Exiting.");
        return Ok(());
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    // Process each article
    for (index, article) in articles.iter().enumerate() {
        let article_id: i64 = article.get("id");
        let analysis_json: String = article.get("analysis");

        // Parse the JSON to extract article text and pub date
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&analysis_json) {
            let article_text = json["summary"].as_str().unwrap_or("");
            let pub_date = json["pub_date"].as_str();

            info!(
                "Processing article ID: {} ({}/{}) - {} characters",
                article_id,
                index + 1,
                total_articles,
                article_text.len()
            );

            if article_text.is_empty() {
                warn!("Empty article text for ID: {}, skipping", article_id);
                failure_count += 1;
                continue;
            }

            // Extract entities
            let start_time = Instant::now();
            match extract_entities(article_text, pub_date, &mut llm_params, &worker_detail).await {
                Ok(extracted_entities) => {
                    info!(
                        "Article ID {}: Extracted {} entities in {:?}",
                        article_id,
                        extracted_entities.entities.len(),
                        start_time.elapsed()
                    );

                    // Convert to JSON for database storage
                    let entities_json = serde_json::to_string(&extracted_entities)
                        .unwrap_or_else(|_| "{}".to_string());

                    // Save to database
                    match db
                        .process_entity_extraction(article_id, &entities_json)
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "Successfully processed entity extraction for article ID: {} with {} entities",
                                article_id,
                                extracted_entities.entities.len()
                            );
                            success_count += 1;
                        }
                        Err(e) => {
                            error!(
                                "Failed to process entity extraction for article ID {}: {:?}",
                                article_id, e
                            );
                            failure_count += 1;
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to extract entities for article ID {}: {:?}",
                        article_id, e
                    );
                    failure_count += 1;
                }
            }

            // Sleep briefly to avoid overwhelming the LLM
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        } else {
            error!(
                "Failed to parse analysis JSON for article ID: {}",
                article_id
            );
            failure_count += 1;
        }
    }

    info!(
        "Processing completed. Success: {}, Failure: {}, Total: {}",
        success_count, failure_count, total_articles
    );

    if success_count == 0 && total_articles > 0 {
        error!("All operations failed. Check logs for details.");
        return Err("All operations failed".into());
    }

    info!("If you have more articles to process, run this tool again with:");
    info!(
        "cargo run --bin process_entities {} {}",
        limit,
        start_from + total_articles as i64
    );

    Ok(())
}
