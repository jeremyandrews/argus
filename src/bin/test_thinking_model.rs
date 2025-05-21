use argus::{
    JsonLLMParams, JsonSchemaType, LLMClient, LLMParamsBase, TextLLMParams, ThinkingModelConfig,
    WorkerDetail,
};
use clap::Parser;
use ollama_rs::{
    generation::completion::request::GenerationRequest, generation::options::GenerationOptions,
    Ollama,
};
use regex::Regex;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[clap(about = "Test the thinking model capabilities")]
struct Args {
    /// Ollama host (just the hostname, not the URL)
    #[clap(short = 'H', long, default_value = "localhost")]
    host: String,

    /// Ollama port
    #[clap(short = 'p', long, default_value = "11434")]
    port: u16,

    /// Thinking model to use
    #[clap(short, long, default_value = "qwen3:30b-a3b-fp16")]
    model: String,

    /// Test prompt to send
    #[clap(
        short = 'P',
        long,
        default_value = "Analyze this text for sentiment and explain your reasoning: 'Today was a great day, but the weather could have been better.'"
    )]
    prompt: String,

    /// Temperature parameter
    #[clap(short = 'T', long, default_value = "0.6")]
    temperature: f32,

    /// Top P parameter
    #[clap(long, default_value = "0.95")]
    top_p: f32,

    /// Top K parameter
    #[clap(long, default_value = "20")]
    top_k: i32,

    /// Enable no-think mode (appends /no_think to prompt)
    #[clap(long)]
    no_think: bool,

    /// Show raw response before stripping thinking tags
    #[clap(short = 'r', long)]
    show_raw: bool,

    /// Enable JSON formatting (simple generic JSON)
    #[clap(short = 'j', long)]
    json: bool,

    /// Use structured JSON with schema (entity, threat, generic)
    #[clap(short = 's', long, default_value = "generic")]
    schema: String,
}

/// Strips <think>...</think> tags from text.
///
/// This is a local copy of the function from src/llm.rs to allow direct comparison
/// of raw and processed responses.
fn strip_thinking_tags(text: &str) -> String {
    // Create a regex pattern to match <think>...</think> blocks
    // Use (?s) to make dot match newlines
    let pattern = r"(?s)<think>.*?</think>";
    let re = Regex::new(pattern).unwrap_or_else(|e| {
        error!("Failed to compile thinking tags regex pattern: {}", e);
        Regex::new(r"nevermatchanything").unwrap()
    });

    // Replace matches with empty string and trim the result
    let result = re.replace_all(text, "").trim().to_string();

    // If the result is empty after stripping, return the original text
    if result.is_empty() {
        return text.to_string();
    }

    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse arguments
    let args = Args::parse();

    // Create Ollama client
    let base_url = if args.host.starts_with("http://") || args.host.starts_with("https://") {
        args.host.clone()
    } else {
        format!("http://{}", args.host)
    };

    info!("Connecting to Ollama at: {}:{}", base_url, args.port);
    let ollama_client = Ollama::new(base_url, args.port);

    // Create worker detail for logging
    let worker_detail = WorkerDetail {
        name: "test thinking".to_string(),
        id: 0,
        model: args.model.clone(),
        connection_info: format!("{}:{}", args.host, args.port),
    };

    info!("Testing thinking model with prompt: {}", args.prompt);
    info!(
        "Parameters: temperature={}, top_p={}, top_k={}",
        args.temperature, args.top_p, args.top_k
    );

    if args.show_raw {
        // When showing raw response, use the direct Ollama API to get the raw response
        let mut request = GenerationRequest::new(args.model.clone(), args.prompt.clone());

        // Apply JSON formatting if specified
        if args.json {
            // We'll use the Format property directly in raw mode
            info!("Enabling JSON format in raw mode");

            // Import the necessary types just for the raw mode
            use ollama_rs::generation::parameters::{FormatType, JsonStructure};

            // Apply the correct JSON format based on schema
            match args.schema.to_lowercase().as_str() {
                "entity" => {
                    info!("Using EntityExtraction JSON schema");
                    request.format = Some(FormatType::Json);
                }
                "threat" => {
                    info!("Using ThreatLocation JSON schema with structure");
                    use argus::llm::ThreatLocationResponse;
                    request.format = Some(FormatType::StructuredJson(JsonStructure::new::<
                        ThreatLocationResponse,
                    >()));
                }
                _ => {
                    info!("Using Generic JSON format");
                    request.format = Some(FormatType::Json);
                }
            };
        }

        // Create prompt, potentially with /no_think
        let prompt_to_use = if args.no_think {
            info!("No-think mode enabled - appending /no_think to prompt");
            format!("{} /no_think", args.prompt)
        } else {
            args.prompt.clone()
        };

        // Create fresh request with the appropriate prompt
        let mut request = GenerationRequest::new(args.model.clone(), prompt_to_use);

        // Configure options
        let options = GenerationOptions::default()
            .temperature(args.temperature)
            .top_p(args.top_p)
            .top_k(args.top_k as u32)
            .num_ctx(8192);

        request.options = Some(options);

        // Apply JSON formatting if needed
        if args.json {
            // We'll use the Format property directly in raw mode
            info!("Enabling JSON format in raw mode");

            // Import the necessary types just for the raw mode
            use ollama_rs::generation::parameters::{FormatType, JsonStructure};

            // Apply the correct JSON format based on schema
            match args.schema.to_lowercase().as_str() {
                "entity" => {
                    info!("Using EntityExtraction JSON schema");
                    request.format = Some(FormatType::Json);
                }
                "threat" => {
                    info!("Using ThreatLocation JSON schema with structure");
                    use argus::llm::ThreatLocationResponse;
                    request.format = Some(FormatType::StructuredJson(JsonStructure::new::<
                        ThreatLocationResponse,
                    >()));
                }
                _ => {
                    info!("Using Generic JSON format");
                    request.format = Some(FormatType::Json);
                }
            };
        }

        info!("Sending direct request to Ollama API...");

        match timeout(Duration::from_secs(120), ollama_client.generate(request)).await {
            Ok(Ok(response)) => {
                let raw_response = response.response;

                // Check for thinking tags
                let contains_tags = raw_response.contains("<think>");

                info!("Raw response contains thinking tags: {}", contains_tags);
                info!("Raw response from model:");
                println!(
                    "\n---RAW RESPONSE START---\n{}\n---RAW RESPONSE END---\n",
                    raw_response
                );

                // Also show processed version
                let processed = strip_thinking_tags(&raw_response);

                if processed != raw_response {
                    info!("Processed response (tags stripped):");
                    println!(
                        "\n---PROCESSED RESPONSE START---\n{}\n---PROCESSED RESPONSE END---\n",
                        processed
                    );
                } else if contains_tags {
                    // This shouldn't happen if contains_tags is true
                    error!("Failed to strip thinking tags that were detected!");
                } else {
                    info!("No thinking tags found to strip.");
                }

                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error generating Ollama response: {}", e);
                Err(anyhow::anyhow!("Error from Ollama API: {}", e))
            }
            Err(_) => {
                error!("Request to Ollama timed out");
                Err(anyhow::anyhow!("Request timed out"))
            }
        }
    } else {
        // When not showing raw, use the standard argus flow
        // Determine JSON format if requested
        let json_format = if args.json {
            // Parse schema string into JsonSchemaType
            let schema_type = match args.schema.to_lowercase().as_str() {
                "entity" => {
                    info!("Using EntityExtraction JSON schema");
                    Some(JsonSchemaType::EntityExtraction)
                }
                "threat" => {
                    info!("Using ThreatLocation JSON schema");
                    Some(JsonSchemaType::ThreatLocation)
                }
                _ => {
                    info!("Using Generic JSON format");
                    Some(JsonSchemaType::Generic)
                }
            };
            schema_type
        } else {
            None
        };

        // Create base LLM params with thinking config
        let base_params = LLMParamsBase {
            llm_client: LLMClient::Ollama(ollama_client),
            model: args.model.clone(),
            temperature: args.temperature,
            thinking_config: if args.no_think {
                // In no_think mode, we still need the thinking_config for compatibility
                // but it won't actually be used
                Some(ThinkingModelConfig {
                    strip_thinking_tags: true,
                    top_p: args.top_p,
                    top_k: args.top_k,
                    min_p: 0.0,
                })
            } else {
                Some(ThinkingModelConfig {
                    strip_thinking_tags: true,
                    top_p: args.top_p,
                    top_k: args.top_k,
                    min_p: 0.0, // Not supported in current ollama-rs version
                })
            },
            no_think: args.no_think, // Use the CLI arg to enable/disable no_think mode
        };

        if args.no_think {
            info!("No-think mode enabled - standard parameters will be used instead of thinking parameters");
        }

        // Create clones of the base params for each response type
        // This avoids the type mismatch error since each branch gets its own variable
        let response;

        if args.json {
            // Create JSON params and use generate_json_response
            let json_base = base_params.clone();
            let json_params = JsonLLMParams {
                base: json_base,
                schema_type: json_format.unwrap_or(JsonSchemaType::Generic),
            };
            response =
                argus::llm::generate_json_response(&args.prompt, &json_params, &worker_detail)
                    .await;
        } else {
            // Create Text params and use generate_text_response
            let text_base = base_params;
            let text_params = TextLLMParams { base: text_base };
            response =
                argus::llm::generate_text_response(&args.prompt, &text_params, &worker_detail)
                    .await;
        }

        match response {
            Some(response) => {
                info!("Response from thinking model:");
                println!("\n{}", response);
                Ok(())
            }
            None => {
                error!("Failed to get response from thinking model");
                Err(anyhow::anyhow!(
                    "Failed to get response from thinking model"
                ))
            }
        }
    }
}
