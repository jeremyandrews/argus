//! # No-Think Mode Test Utility
//!
//! This utility tests the no-think mode functionality with Qwen models.
//!
//! ## Usage
//!
//! ```
//! # Test with no-think mode enabled
//! cargo run --bin test_no_think -- --model qwen3:32b-a3b-fp16 --no-think
//!
//! # Compare with standard thinking mode
//! cargo run --bin test_no_think -- --model qwen3:32b-a3b-fp16
//! ```
//!
//! This will run a test with both modes for comparison.

use argus::{LLMClient, LLMParamsBase, TextLLMParams, WorkerDetail};
use clap::Parser;
use ollama_rs::Ollama;
use std::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[clap(about = "Test the no-think mode for Qwen models")]
struct Args {
    /// Ollama host
    #[clap(long, default_value = "localhost")]
    host: String,

    /// Ollama port
    #[clap(long, default_value = "11434")]
    port: u16,

    /// Model to use (should be a Qwen model for no-think to work)
    #[clap(long, default_value = "qwen3:32b-a3b-fp16")]
    model: String,

    /// Enable no-think mode
    #[clap(long)]
    no_think: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse command-line arguments
    let args = Args::parse();

    // Test prompt that normally triggers thinking
    let test_prompt = "Explain why quantum computing is important for cryptography. Consider both the advantages and potential risks.";

    // Create Ollama client
    let ollama_client = Ollama::new(args.host.clone(), args.port);
    let llm_client = LLMClient::Ollama(ollama_client);

    // Create worker detail
    let worker_detail = WorkerDetail {
        name: "no-think test".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    // Create LLM params
    let llm_params = TextLLMParams {
        base: LLMParamsBase {
            llm_client,
            model: args.model.clone(),
            temperature: 0.6,
            thinking_config: Some(argus::ThinkingModelConfig {
                strip_thinking_tags: true,
                top_p: 0.95,
                top_k: 20,
                min_p: 0.0,
            }),
            no_think: args.no_think,
        },
    };

    // Log mode
    if args.no_think {
        info!("Running in NO-THINK mode (/no_think will be appended to prompt)");
    } else {
        info!("Running in standard thinking mode");
    }

    // Generate response
    info!("Sending prompt: {}", test_prompt);
    let start = Instant::now();

    match argus::llm::generate_text_response(test_prompt, &llm_params, &worker_detail).await {
        Some(response) => {
            let elapsed = start.elapsed();
            info!("Response received in {:?}:", elapsed);
            println!("\n---RESPONSE---\n{}\n---END RESPONSE---", response);

            // Provide feedback on response characteristics
            let contains_thinking_tags = response.contains("<think>");
            info!(
                "Response contains thinking tags: {}",
                contains_thinking_tags
            );
            info!(
                "Response length: {} characters, {} words",
                response.len(),
                response.split_whitespace().count()
            );
        }
        None => {
            eprintln!("Failed to get response!");
        }
    }

    Ok(())
}
