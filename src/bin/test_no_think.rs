use argus::{JsonLLMParams, JsonSchemaType, LLMClient, LLMParamsBase, ModelConfig, WorkerDetail};
use clap::Parser;
use ollama_rs::Ollama;
use serde_json::to_string_pretty;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host for the Ollama server
    #[arg(short = 'H', long, default_value = "localhost")]
    host: String,

    /// Port for the Ollama server
    #[arg(short = 'p', long, default_value = "11434")]
    port: u16,

    /// Model to use
    #[arg(short = 'm', long, default_value = "llama3:8b")]
    model: String,

    /// Temperature for generation
    #[arg(short = 'T', long, default_value = "0.0")]
    temperature: f32,

    /// Enable no-think mode
    #[arg(long)]
    no_think: bool,

    /// Test article text
    #[arg(short = 'a', long)]
    article: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Create Ollama client and wrap it in LLMClient
    let ollama = Ollama::new(args.host.clone(), args.port);
    let llm_client = LLMClient::Ollama(ollama);

    // Setup worker detail
    let worker_detail = WorkerDetail {
        name: "no_think_test_worker".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    // Default article text if none provided
    let article_text = args.article.unwrap_or_else(|| {
        "Apple Inc. announced today the launch of their new iPad Pro featuring the M4 chip. CEO Tim Cook revealed the device at a special event in Cupertino, California. The new tablet will be available starting next week at Apple Stores worldwide, with prices beginning at $999. Industry experts predict strong sales despite the premium pricing due to significant performance improvements over the previous generation.".to_string()
    });

    println!("Testing no-think mode with:");
    println!("- Model: {}", args.model);
    println!("- Temperature: {}", args.temperature);
    println!("- No-think enabled: {}", args.no_think);
    println!("- Article: {} characters", article_text.len());

    // Create model config with appropriate parameters for no-think mode
    let model_config = if args.no_think {
        Some(ModelConfig {
            strip_thinking_tags: true,
            top_p: 0.8,
            top_k: 20,
            min_p: 0.0,
        })
    } else {
        Some(ModelConfig {
            strip_thinking_tags: true,
            top_p: 0.95,
            top_k: 20,
            min_p: 0.0,
        })
    };

    // Create LLM params for entity extraction
    let mut llm_params = JsonLLMParams {
        base: LLMParamsBase {
            llm_client,
            model: args.model.clone(),
            temperature: args.temperature,
            model_config,
            no_think: args.no_think,
            context_window: None,
        },
        schema_type: JsonSchemaType::EntityExtraction,
    };

    println!("\nTesting entity extraction with no-think mode...");
    let start_time = std::time::Instant::now();

    // Extract entities to test no-think mode
    match argus::entity::extraction::extract_entities(
        &article_text,
        Some("2024-01-15"),
        &mut llm_params,
        &worker_detail,
    )
    .await
    {
        Ok(extraction_result) => {
            let elapsed = start_time.elapsed();

            println!("\nEntity extraction completed in {:.2?}", elapsed);
            println!("Extracted {} entities", extraction_result.entities.len());

            // Print results
            println!("\n=== EXTRACTION RESULTS ===");
            println!("{}", to_string_pretty(&extraction_result)?);

            // Verify results
            if extraction_result.entities.is_empty() {
                println!("\n❌ TEST FAILED: No entities extracted");
                return Err("No entities extracted".into());
            }

            // Check for specific expected entities from the test article
            let entity_names: Vec<&str> = extraction_result
                .entities
                .iter()
                .map(|e| e.name.as_str())
                .collect();

            let expected_entities = ["Apple", "Tim Cook", "iPad Pro", "M4", "Cupertino"];
            let mut found_entities = Vec::new();

            for expected in &expected_entities {
                if entity_names.iter().any(|name| name.contains(expected)) {
                    found_entities.push(*expected);
                }
            }

            println!("\nExpected entities found: {:?}", found_entities);

            if found_entities.len() >= 3 {
                println!("\n✅ TEST PASSED: No-think mode entity extraction successful!");
                println!(
                    "Found {} out of {} expected entities",
                    found_entities.len(),
                    expected_entities.len()
                );
            } else {
                println!(
                    "\n⚠️  WARNING: Only found {} out of {} expected entities",
                    found_entities.len(),
                    expected_entities.len()
                );
                println!(
                    "This might indicate the model is not performing optimally with no-think mode"
                );
            }

            if args.no_think {
                println!("\n✅ No-think mode test completed successfully!");
            } else {
                println!("\n✅ Regular mode test completed successfully!");
            }
        }
        Err(e) => {
            println!("\n❌ TEST FAILED: Error during entity extraction: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
