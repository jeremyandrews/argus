use anyhow::Result;
use argus::environment;
use ollama_rs::Ollama;
use std::{collections::HashMap, fmt, time::Duration};
use tokio::time::timeout;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use url::Url;

/// Struct to hold Ollama endpoint configuration
#[derive(Debug)]
struct OllamaConfig {
    url: String,
    models: Vec<String>,
}

/// Enum to represent the status of an endpoint
#[derive(Debug)]
enum EndpointStatus {
    Up(Vec<String>), // Available models
    Down(String),    // Error message
}

impl fmt::Display for EndpointStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointStatus::Up(_) => write!(f, "UP"),
            EndpointStatus::Down(_) => write!(f, "DOWN"),
        }
    }
}

/// Parse Ollama configs from environment variable
fn parse_ollama_configs(env_var_name: &str) -> Vec<OllamaConfig> {
    let configs = environment::get_env_var_as_vec(env_var_name, ',');
    let mut result = Vec::new();

    for config in configs {
        if config.is_empty() {
            continue;
        }

        let parts: Vec<&str> = config.split('@').collect();
        if parts.len() != 2 {
            error!("Invalid config format: {}", config);
            continue;
        }

        let models: Vec<String> = parts[0].split('/').map(String::from).collect();
        let url = parts[1].to_string();

        if !url.starts_with("http://") && !url.starts_with("https://") {
            // Add http:// prefix if missing
            let url = format!("http://{}", url);
            result.push(OllamaConfig { url, models });
        } else {
            result.push(OllamaConfig { url, models });
        }
    }

    result
}

/// Test an Ollama endpoint and return its status
async fn test_endpoint(config: &OllamaConfig) -> EndpointStatus {
    // Parse URL to extract host and port
    let url = match Url::parse(&config.url) {
        Ok(url) => url,
        Err(e) => return EndpointStatus::Down(format!("Invalid URL: {}", e)),
    };

    let host = url.host_str().unwrap_or("localhost").to_string();
    let port = url.port().unwrap_or(11434);

    info!("Testing Ollama at {}:{}", host, port);

    // Create Ollama client
    let ollama = Ollama::new(host, port);

    match timeout(Duration::from_secs(10), ollama.list_local_models()).await {
        Ok(Ok(models)) => {
            let available_models: Vec<String> = models.iter().map(|m| m.name.clone()).collect();

            EndpointStatus::Up(available_models)
        }
        Ok(Err(e)) => EndpointStatus::Down(format!("API error: {}", e)),
        Err(_) => EndpointStatus::Down("Connection timed out".to_string()),
    }
}

/// Test all endpoints and return their statuses
async fn test_endpoints(configs: &[OllamaConfig]) -> HashMap<String, EndpointStatus> {
    let mut results = HashMap::new();

    for config in configs {
        let status = test_endpoint(config).await;
        results.insert(config.url.clone(), status);
    }

    results
}

/// Print the endpoint status report
fn print_report(
    worker_type: &str,
    configs: &[OllamaConfig],
    results: &HashMap<String, EndpointStatus>,
) {
    println!("\n{} WORKERS", worker_type);
    println!("{}", "-".repeat(worker_type.len() + 8));

    let mut up_count = 0;
    let mut missing_models: HashMap<String, Vec<String>> = HashMap::new();

    for config in configs {
        match results.get(&config.url) {
            Some(EndpointStatus::Up(available_models)) => {
                up_count += 1;
                println!("✅ {} - UP", config.url);

                // Check which expected models are available
                let mut available_count = 0;

                for model in &config.models {
                    if available_models.contains(model) {
                        available_count += 1;
                        println!("  ✅ AVAILABLE: {}", model);
                    } else {
                        println!("  ❌ MISSING: {}", model);

                        // Track missing models for the summary
                        missing_models
                            .entry(model.clone())
                            .or_insert_with(Vec::new)
                            .push(config.url.clone());
                    }
                }

                // Also list models that are available but not in our config
                let unexpected_models: Vec<&String> = available_models
                    .iter()
                    .filter(|m| !config.models.contains(m))
                    .collect();

                if !unexpected_models.is_empty() {
                    println!("  ℹ️ ADDITIONAL MODELS:");
                    for model in unexpected_models {
                        println!("    - {}", model);
                    }
                }

                println!(
                    "  📊 {}/{} configured models available",
                    available_count,
                    config.models.len()
                );
            }
            Some(EndpointStatus::Down(error)) => {
                println!("❌ {} - DOWN", config.url);
                println!("  ⚠️ Error: {}", error);

                // Track all models as missing for this endpoint
                for model in &config.models {
                    missing_models
                        .entry(model.clone())
                        .or_insert_with(Vec::new)
                        .push(config.url.clone());
                }
            }
            None => {
                println!("❓ {} - UNKNOWN (not tested)", config.url);
            }
        }

        println!();
    }

    // Print summary stats for this worker type
    println!("📋 Summary: {}/{} endpoints UP", up_count, configs.len());

    if !configs.is_empty() {
        let percentage = (up_count as f64 / configs.len() as f64) * 100.0;
        println!("   {:.1}% availability", percentage);
    }
}

/// Print a summary of all the results
fn print_summary(
    decision_configs: &[OllamaConfig],
    decision_results: &HashMap<String, EndpointStatus>,
    analysis_configs: &[OllamaConfig],
    analysis_results: &HashMap<String, EndpointStatus>,
) {
    println!("\nOVERALL SUMMARY");
    println!("--------------");

    // Count up endpoints
    let decision_up = decision_results
        .values()
        .filter(|s| matches!(s, EndpointStatus::Up(_)))
        .count();

    let analysis_up = analysis_results
        .values()
        .filter(|s| matches!(s, EndpointStatus::Up(_)))
        .count();

    println!(
        "Decision endpoints: {}/{} UP",
        decision_up,
        decision_configs.len()
    );
    println!(
        "Analysis endpoints: {}/{} UP",
        analysis_up,
        analysis_configs.len()
    );

    // Collect missing models across all endpoints
    let mut all_missing_models: HashMap<String, Vec<String>> = HashMap::new();

    for config in decision_configs.iter().chain(analysis_configs.iter()) {
        if let Some(EndpointStatus::Up(available_models)) = decision_results
            .get(&config.url)
            .or_else(|| analysis_results.get(&config.url))
        {
            for model in &config.models {
                if !available_models.contains(model) {
                    all_missing_models
                        .entry(model.clone())
                        .or_insert_with(Vec::new)
                        .push(config.url.clone());
                }
            }
        } else {
            // If the endpoint is down, all models are missing
            for model in &config.models {
                all_missing_models
                    .entry(model.clone())
                    .or_insert_with(Vec::new)
                    .push(config.url.clone());
            }
        }
    }

    if all_missing_models.is_empty() {
        println!("🎉 All configured models are available on their respective endpoints!");
    } else {
        println!("\nMissing models:");
        for (model, endpoints) in &all_missing_models {
            println!("- {} missing from {} endpoints:", model, endpoints.len());
            for endpoint in endpoints {
                println!("  - {}", endpoint);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    println!("OLLAMA ENDPOINT STATUS REPORT");
    println!("=============================");

    // Parse configurations
    let decision_configs = parse_ollama_configs("DECISION_OLLAMA_CONFIGS");
    let analysis_configs = parse_ollama_configs("ANALYSIS_OLLAMA_CONFIGS");

    info!(
        "Testing {} decision endpoints and {} analysis endpoints",
        decision_configs.len(),
        analysis_configs.len()
    );

    if decision_configs.is_empty() && analysis_configs.is_empty() {
        println!("\n⚠️ No Ollama configurations found in environment variables.");
        println!("Please set DECISION_OLLAMA_CONFIGS and/or ANALYSIS_OLLAMA_CONFIGS");
        println!("Format: model1/model2@host:port,model3@host2:port2");
        return Ok(());
    }

    // Test all endpoints
    let decision_results = test_endpoints(&decision_configs).await;
    let analysis_results = test_endpoints(&analysis_configs).await;

    // Generate and print reports
    if !decision_configs.is_empty() {
        print_report("DECISION", &decision_configs, &decision_results);
    }

    if !analysis_configs.is_empty() {
        print_report("ANALYSIS", &analysis_configs, &analysis_results);
    }

    // Print overall summary
    print_summary(
        &decision_configs,
        &decision_results,
        &analysis_configs,
        &analysis_results,
    );

    // Determine if we should exit with an error
    let any_down = decision_results
        .values()
        .chain(analysis_results.values())
        .any(|status| matches!(status, EndpointStatus::Down(_)));

    if any_down {
        // Non-zero exit code indicates there was an issue
        std::process::exit(1);
    }

    Ok(())
}
