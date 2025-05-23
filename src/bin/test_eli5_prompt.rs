use argus::llm::generate_text_response;
use argus::prompt::eli5_prompt;
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

    /// File path to test content
    #[arg(short = 'f', long)]
    file: Option<String>,

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
    let article_text = if let Some(file_path) = args.file {
        let mut file = std::fs::File::open(file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        content
    } else {
        "Apple has announced the new iPhone 15 Pro with significant camera improvements, including a 48MP main camera with a larger sensor. The device will be available starting September 22 with a base price of $999. Industry analysts expect strong sales despite the premium pricing.".to_string()
    };

    println!("\nTesting ELI5 prompt with article text:");
    println!("-------------------------------------");
    println!("{}", article_text);
    println!("-------------------------------------\n");

    // Generate ELI5 explanation
    let eli5_prompt_text = eli5_prompt(&article_text, None);

    println!("Generated ELI5 prompt (abbreviated):");
    println!("----------------------------------");
    let shortened_prompt = eli5_prompt_text
        .lines()
        .take(20)
        .collect::<Vec<&str>>()
        .join("\n");
    println!("{}...\n[Prompt continues]", shortened_prompt);

    // Get response from LLM
    println!(
        "\nGenerating ELI5 explanation with model: {}...",
        args.model
    );

    let start_time = std::time::Instant::now();
    let eli5_response =
        generate_text_response(&eli5_prompt_text, &text_params, &worker_detail).await;
    let elapsed = start_time.elapsed();

    // Display results
    if let Some(response) = eli5_response {
        println!("\nELI5 Explanation (generated in {:.2?}):", elapsed);
        println!("-------------------------------------");
        println!("{}", response);
        println!("-------------------------------------");
        println!("\nTest completed successfully ✅");
    } else {
        println!("\nError: Failed to generate ELI5 explanation ❌");
    }

    // Also verify that the response is included in the analysis process
    println!("\nNote: This test only verifies prompt generation and response.");
    println!("To verify integration with the full analysis pipeline, use:");
    println!("cargo run --bin test_full_analysis -- -a \"path/to/article.txt\"");

    Ok(())
}
