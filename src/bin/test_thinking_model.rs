use argus::{LLMClient, LLMParamsBase, ModelConfig, TextLLMParams, WorkerDetail};
use clap::Parser;
use ollama_rs::Ollama;
use std::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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
    #[arg(short = 'm', long, default_value = "qwen2.5:7b")]
    model: String,

    /// Temperature for generation
    #[arg(short = 'T', long, default_value = "0.6")]
    temperature: f32,

    /// Enable no-think mode
    #[arg(long)]
    no_think: bool,

    /// TopP setting
    #[arg(long, default_value = "0.95")]
    top_p: f32,

    /// TopK setting
    #[arg(long, default_value = "20")]
    top_k: i32,

    /// MinP setting
    #[arg(long, default_value = "0.0")]
    min_p: f32,

    /// Test prompt
    #[arg(short = 'p', long)]
    prompt: Option<String>,

    /// Number of test iterations
    #[arg(short = 'i', long, default_value = "1")]
    iterations: usize,

    /// Enable verbose logging
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(if args.verbose {
            Level::DEBUG
        } else {
            Level::INFO
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting thinking model test");

    // Create Ollama client
    let ollama = Ollama::new(args.host.clone(), args.port);
    let llm_client = LLMClient::Ollama(ollama);

    // Setup worker detail
    let worker_detail = WorkerDetail {
        name: "thinking_test_worker".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    // Default test prompt
    let test_prompt = args.prompt.unwrap_or_else(|| {
        "Analyze the following scenario and provide a thoughtful response: A tech company is considering whether to implement a 4-day work week. What are the potential benefits and drawbacks of this decision, and what factors should they consider?".to_string()
    });

    println!("Testing thinking model with:");
    println!("- Model: {}", args.model);
    println!("- Temperature: {}", args.temperature);
    println!("- No-think mode: {}", args.no_think);
    println!("- TopP: {}", args.top_p);
    println!("- TopK: {}", args.top_k);
    println!("- MinP: {}", args.min_p);
    println!("- Iterations: {}", args.iterations);
    println!("- Prompt length: {} characters", test_prompt.len());

    let mut total_time = std::time::Duration::new(0, 0);
    let mut successful_responses = 0;

    for iteration in 1..=args.iterations {
        println!("\n--- Iteration {} ---", iteration);

        // Create model config
        let model_config = Some(ModelConfig {
            strip_thinking_tags: true,
            top_p: args.top_p,
            top_k: args.top_k,
            min_p: args.min_p,
        });

        // Create LLM params
        let mut llm_params = TextLLMParams {
            base: LLMParamsBase {
                llm_client: llm_client.clone(),
                model: args.model.clone(),
                temperature: args.temperature,
                model_config: if args.no_think { None } else { model_config },
                no_think: args.no_think,
            },
        };

        let start_time = Instant::now();

        // Generate response
        match argus::llm::generate_text_response(&test_prompt, &mut llm_params, &worker_detail)
            .await
        {
            Some(response) => {
                let elapsed = start_time.elapsed();
                total_time += elapsed;
                successful_responses += 1;

                println!("Response generated in {:.2?}", elapsed);
                println!("Response length: {} characters", response.len());

                if args.verbose {
                    println!("\n=== RESPONSE ===");
                    println!("{}", response);
                    println!("=== END RESPONSE ===");
                }

                // Basic quality checks
                if response.is_empty() {
                    println!("⚠️  WARNING: Empty response generated");
                    continue;
                }

                if response.len() < 100 {
                    println!(
                        "⚠️  WARNING: Very short response ({} characters)",
                        response.len()
                    );
                }

                // Check for thinking tags in the response (should be stripped)
                if response.contains("<thinking>") || response.contains("</thinking>") {
                    println!("⚠️  WARNING: Thinking tags found in response - stripping may not be working");
                }

                // Check for structured thinking in thinking models
                if !args.no_think {
                    let has_structure = response.contains("benefits")
                        || response.contains("drawbacks")
                        || response.contains("advantages")
                        || response.contains("disadvantages")
                        || response.contains("considerations");

                    if has_structure {
                        println!("✅ Response shows structured analysis");
                    } else {
                        println!("⚠️  Response may lack structured analysis");
                    }
                }

                println!("✅ Iteration {} completed successfully", iteration);
            }
            None => {
                let elapsed = start_time.elapsed();
                println!(
                    "❌ Iteration {} failed after {:.2?} - no response generated",
                    iteration, elapsed
                );
            }
        }

        // Brief pause between iterations
        if iteration < args.iterations {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
    }

    // Summary
    println!("\n=== TEST SUMMARY ===");
    println!("Total iterations: {}", args.iterations);
    println!("Successful responses: {}", successful_responses);
    println!(
        "Success rate: {:.1}%",
        (successful_responses as f64 / args.iterations as f64) * 100.0
    );

    if successful_responses > 0 {
        let avg_time = total_time / successful_responses as u32;
        println!("Average response time: {:.2?}", avg_time);
    }

    if successful_responses == args.iterations {
        println!("\n✅ ALL TESTS PASSED: Thinking model is working correctly!");
    } else if successful_responses > 0 {
        println!(
            "\n⚠️  PARTIAL SUCCESS: {}/{} tests passed",
            successful_responses, args.iterations
        );
    } else {
        println!("\n❌ ALL TESTS FAILED: No successful responses generated");
        return Err("All tests failed".into());
    }

    if args.no_think {
        println!("\nNote: No-think mode was enabled for this test");
    } else {
        println!("\nNote: Thinking mode was enabled for this test");
    }

    Ok(())
}
