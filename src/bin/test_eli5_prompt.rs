use argus::{LLMClient, LLMParamsBase, TextLLMParams, WorkerDetail};
use clap::Parser;
use ollama_rs::Ollama;

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

    /// Article text to test with
    #[arg(short = 'a', long)]
    article: Option<String>,

    /// Temperature for generation
    #[arg(short = 'T', long, default_value = "0.7")]
    temperature: f32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Create Ollama client and wrap it in LLMClient
    let ollama = Ollama::new(args.host.clone(), args.port);
    let llm_client = LLMClient::Ollama(ollama);

    // Setup worker detail
    let worker_detail = WorkerDetail {
        name: "eli5_test_worker".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    // Default article text if none provided
    let article_text = args.article.unwrap_or_else(|| {
        "Apple announced its quarterly earnings, showing a 5% increase in revenue driven by strong iPhone 15 sales and growing services revenue. The company's stock price rose 3% in after-hours trading as investors responded positively to the better-than-expected results.".to_string()
    });

    println!("Testing ELI5 prompt with:");
    println!("- Model: {}", args.model);
    println!("- Temperature: {}", args.temperature);
    println!("- Article: {} characters", article_text.len());

    // Create LLM params
    let mut llm_params = TextLLMParams {
        base: LLMParamsBase {
            llm_client,
            model: args.model.clone(),
            temperature: args.temperature,
            model_config: None,
            no_think: false,
            context_window: None,
        },
    };

    println!("\nGenerating ELI5 explanation...");
    let start_time = std::time::Instant::now();

    // Generate mock analysis data for testing
    let mock_critical_analysis =
        "Credibility Score: 8/10 - Well-sourced article with clear attribution";
    let mock_logical_fallacies = "No apparent logical fallacies detected";
    let mock_source_analysis =
        "Domain Name: example.com - Established news organization with professional standards";
    let mock_sources_quality = 3u8;
    let mock_argument_quality = 3u8;
    let mock_source_type = "press";

    // Generate ELI5 prompt with analysis context
    let eli5_prompt = argus::prompt::eli5_prompt(
        &article_text,
        None,
        mock_critical_analysis,
        mock_logical_fallacies,
        mock_source_analysis,
        mock_sources_quality,
        mock_argument_quality,
        mock_source_type,
    );

    // Generate ELI5 explanation using the LLM
    let eli5_result =
        argus::llm::generate_text_response(&eli5_prompt, &mut llm_params, &worker_detail).await;

    let elapsed = start_time.elapsed();

    match eli5_result {
        Some(explanation) => {
            println!("\nELI5 explanation generated in {:.2?}", elapsed);
            println!("\n=== ELI5 EXPLANATION ===");
            println!("{}", explanation);

            // Basic validation
            if explanation.is_empty() {
                println!("\n❌ TEST FAILED: Empty explanation generated");
                return Err("Empty explanation".into());
            }

            if explanation.len() < 50 {
                println!(
                    "\n⚠️  WARNING: Explanation seems very short ({} characters)",
                    explanation.len()
                );
            }

            // Check for child-friendly language indicators
            let has_simple_language = explanation.to_lowercase().contains("like")
                || explanation.to_lowercase().contains("imagine")
                || explanation.to_lowercase().contains("think of")
                || explanation.to_lowercase().contains("similar to")
                || explanation.to_lowercase().contains("just like");

            if has_simple_language {
                println!("\n✅ TEST PASSED: ELI5 explanation generated with simple language!");
            } else {
                println!("\n⚠️  WARNING: Explanation might not be using child-friendly language");
            }
        }
        None => {
            println!("\n❌ TEST FAILED: Error generating ELI5 explanation");
            return Err("Failed to generate ELI5 explanation".into());
        }
    }

    Ok(())
}
