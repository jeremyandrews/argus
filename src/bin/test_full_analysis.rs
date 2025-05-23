use argus::workers::analysis::quality::process_analysis;
use argus::{LLMClient, TextLLMParams, WorkerDetail};
use clap::Parser;
use ollama_rs::Ollama;
use std::io::Read;

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

    /// Article text
    #[arg(short = 'a', long)]
    article_file: Option<String>,

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
        name: "test_worker".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    // Create LLM params
    let text_params = TextLLMParams {
        base: argus::LLMParamsBase {
            llm_client,
            model: args.model.clone(),
            temperature: args.temperature,
            thinking_config: None,
            no_think: false,
        },
    };

    // Get article text from file or use sample text
    let article_text = if let Some(file_path) = args.article_file {
        let mut file = std::fs::File::open(file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        content
    } else {
        "Apple has announced the new iPhone 15 Pro with significant camera improvements, including a 48MP main camera with a larger sensor. The device will be available starting September 22 with a base price of $999. Industry analysts expect strong sales despite the premium pricing.".to_string()
    };

    // Mock HTML and URL for testing
    let article_html = format!("<html><body>{}</body></html>", article_text);
    let article_url = "https://example.com/article";

    println!("\nRunning full analysis pipeline test with:");
    println!("- Model: {}", args.model);
    println!("- Temperature: {}", args.temperature);
    println!("- Article: {} characters", article_text.len());

    println!("\nProcessing...");
    let start_time = std::time::Instant::now();

    // Run the full analysis process
    let (
        summary,
        tiny_summary,
        tiny_title,
        critical_analysis,
        logical_fallacies,
        source_analysis,
        relation,
        sources_quality,
        argument_quality,
        source_type,
        additional_insights,
        action_recommendations,
        talking_points,
        eli5,
    ) = process_analysis(
        &article_text,
        &article_html,
        article_url,
        Some("test"),
        None,
        &text_params,
        &worker_detail,
    )
    .await;

    let elapsed = start_time.elapsed();

    // Print results to validate all fields
    println!("\nAnalysis completed in {:.2?}", elapsed);
    println!("\n=== ANALYSIS RESULTS ===");

    println!("\n-- TINY TITLE --");
    println!("{}", tiny_title);

    println!("\n-- TINY SUMMARY --");
    println!("{}", tiny_summary);

    println!("\n-- SUMMARY --");
    println!("{}", summary);

    println!("\n-- CRITICAL ANALYSIS --");
    println!("{}", critical_analysis);

    println!("\n-- LOGICAL FALLACIES --");
    println!("{}", logical_fallacies);

    println!("\n-- SOURCE ANALYSIS --");
    println!("{}", source_analysis);

    println!("\n-- ADDITIONAL INSIGHTS --");
    println!("{}", additional_insights);

    println!("\n-- ACTION RECOMMENDATIONS --");
    println!("{}", action_recommendations);

    println!("\n-- TALKING POINTS --");
    println!("{}", talking_points);

    println!("\n-- SOURCE TYPE --");
    println!("{}", source_type);

    println!("\n-- QUALITY SCORES --");
    println!("Sources Quality: {}", sources_quality);
    println!("Argument Quality: {}", argument_quality);

    // Check if relation is present (depends on topic being provided)
    if let Some(rel) = relation {
        println!("\n-- RELATION TO TOPIC --");
        println!("{}", rel);
    }

    // Finally, check the ELI5 explanation
    println!("\n-- ELI5 EXPLANATION --");
    println!("{}", eli5);

    // Verify all required fields are present
    let mut missing_fields = Vec::new();

    if summary.is_empty() {
        missing_fields.push("summary");
    }
    if tiny_summary.is_empty() {
        missing_fields.push("tiny_summary");
    }
    if tiny_title.is_empty() {
        missing_fields.push("tiny_title");
    }
    if critical_analysis.is_empty() {
        missing_fields.push("critical_analysis");
    }
    if logical_fallacies.is_empty() {
        missing_fields.push("logical_fallacies");
    }
    if source_analysis.is_empty() {
        missing_fields.push("source_analysis");
    }
    if additional_insights.is_empty() {
        missing_fields.push("additional_insights");
    }
    if action_recommendations.is_empty() {
        missing_fields.push("action_recommendations");
    }
    if talking_points.is_empty() {
        missing_fields.push("talking_points");
    }
    if source_type.is_empty() {
        missing_fields.push("source_type");
    }
    if eli5.is_empty() {
        missing_fields.push("eli5");
    }

    if missing_fields.is_empty() {
        println!("\n✅ TEST PASSED: All analysis fields generated successfully!");
    } else {
        println!("\n❌ TEST FAILED: The following fields were missing or empty:");
        for field in missing_fields {
            println!("  - {}", field);
        }
    }

    Ok(())
}
