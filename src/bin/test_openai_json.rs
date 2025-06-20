//! Test OpenAI JSON mode support for entity extraction and threat location analysis

use argus::{JsonLLMParams, JsonSchemaType, LLMClient, LLMParamsBase, WorkerDetail};
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use clap::Parser;

#[derive(Parser)]
#[command(author, version, about = "Test OpenAI JSON mode support", long_about = None)]
struct Args {
    /// OpenAI API key
    #[arg(long)]
    openai_api_key: String,

    /// OpenAI model to test
    #[arg(long, default_value = "gpt-3.5-turbo")]
    model: String,

    /// Test type: entity or threat
    #[arg(long, default_value = "entity", value_parser = ["entity", "threat"])]
    test_type: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Create OpenAI client
    let config = OpenAIConfig::new().with_api_key(args.openai_api_key);
    let openai_client = OpenAIClient::with_config(config);

    // Create LLM params
    let llm_params = JsonLLMParams {
        base: LLMParamsBase {
            llm_client: LLMClient::OpenAI(openai_client),
            model: args.model.clone(),
            temperature: 0.1,
            no_think: false,
            context_window: Some(4096),
            model_config: None,
        },
        schema_type: match args.test_type.as_str() {
            "entity" => JsonSchemaType::EntityExtraction,
            "threat" => JsonSchemaType::ThreatLocation,
            _ => JsonSchemaType::Generic,
        },
    };

    let worker_detail = WorkerDetail {
        name: "test".to_string(),
        id: 1,
        model: args.model.clone(),
        connection_info: "test".to_string(),
    };

    // Test prompts
    let test_prompt = match args.test_type.as_str() {
        "entity" => {
            "President Joe Biden met with Prime Minister Justin Trudeau in Ottawa on March 15, 2024 to discuss trade relations between the United States and Canada."
        }
        "threat" => {
            "A wildfire broke out near Vancouver, British Columbia, forcing evacuations in the surrounding areas including Richmond and Burnaby."
        }
        _ => "Test prompt for generic JSON response."
    };

    println!(
        "Testing OpenAI {} with {} schema...",
        args.model, args.test_type
    );
    println!("Prompt: {}", test_prompt);
    println!();

    // Generate response
    match argus::llm::generate_json_response(test_prompt, &llm_params, &worker_detail).await {
        Some(response) => {
            println!("✅ Success! OpenAI JSON response:");
            println!("{}", response);

            // Try to parse as JSON to verify validity
            match serde_json::from_str::<serde_json::Value>(&response) {
                Ok(json) => {
                    println!("\n✅ Response is valid JSON");
                    if args.test_type == "entity" {
                        if let Some(entities) = json.get("entities") {
                            if entities.is_array() {
                                println!("✅ Contains entities array as expected");
                            } else {
                                println!("⚠️  Warning: entities field is not an array");
                            }
                        } else {
                            println!("⚠️  Warning: Missing entities field");
                        }
                    } else if args.test_type == "threat" {
                        if let Some(regions) = json.get("impacted_regions") {
                            if regions.is_array() {
                                println!("✅ Contains impacted_regions array as expected");
                            } else {
                                println!("⚠️  Warning: impacted_regions field is not an array");
                            }
                        } else {
                            println!("⚠️  Warning: Missing impacted_regions field");
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Error: Response is not valid JSON: {}", e);
                }
            }
        }
        None => {
            println!("❌ Failed to get response from OpenAI");
        }
    }

    Ok(())
}
