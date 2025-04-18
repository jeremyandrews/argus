use anyhow::Result;
use argus::{process_analysis_ollama_configs, process_ollama_configs};
use ollama_rs::Ollama;
use std::{collections::HashMap, env, time::Duration};
use tokio::time::timeout;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

const DECISION_OLLAMA_CONFIGS_ENV: &str = "DECISION_OLLAMA_CONFIGS";
const ANALYSIS_OLLAMA_CONFIGS_ENV: &str = "ANALYSIS_OLLAMA_CONFIGS";
const CONNECTION_TIMEOUT_SECS: u64 = 10;

/// Struct to hold endpoint status
#[derive(Debug)]
enum EndpointStatus {
    Up(Vec<String>), // Available models
    Down(String),    // Error message
}

/// Test specific Ollama endpoint and check available models
async fn test_endpoint(host: &str, port: u16) -> EndpointStatus {
    info!("Testing Ollama endpoint at {}:{}", host, port);

    // Create Ollama client - this returns a client directly, not a Result
    let ollama = Ollama::new(host.to_string(), port);

    // Attempt to list available models
    match timeout(
        Duration::from_secs(CONNECTION_TIMEOUT_SECS),
        ollama.list_local_models(),
    )
    .await
    {
        Ok(Ok(models)) => {
            let available_models: Vec<String> = models.iter().map(|m| m.name.clone()).collect();

            EndpointStatus::Up(available_models)
        }
        Ok(Err(e)) => EndpointStatus::Down(format!("API error: {}", e)),
        Err(_) => EndpointStatus::Down("Connection timed out".to_string()),
    }
}

/// Test all Decision worker endpoints
async fn test_decision_endpoints(
    configs: &[(String, u16, String)],
) -> HashMap<String, EndpointStatus> {
    let mut results = HashMap::new();

    for (host, port, _) in configs {
        let endpoint_key = format!("{}:{}", host, port);
        let status = test_endpoint(host, *port).await;
        results.insert(endpoint_key, status);
    }

    results
}

/// Test all Analysis worker endpoints (including fallbacks)
async fn test_analysis_endpoints(
    configs: &[(String, u16, String, Option<(String, u16, String)>)],
) -> HashMap<String, EndpointStatus> {
    let mut results = HashMap::new();

    // Test main endpoints
    for (host, port, _, fallback) in configs {
        let endpoint_key = format!("{}:{}", host, port);
        let status = test_endpoint(host, *port).await;
        results.insert(endpoint_key, status);

        // Also test fallback endpoint if available
        if let Some((fallback_host, fallback_port, _)) = fallback {
            let fallback_key = format!("{}:{} (fallback)", fallback_host, fallback_port);
            let fallback_status = test_endpoint(fallback_host, *fallback_port).await;
            results.insert(fallback_key, fallback_status);
        }
    }

    results
}

/// Print report for Decision worker endpoints
fn print_decision_report(
    configs: &[(String, u16, String)],
    results: &HashMap<String, EndpointStatus>,
) {
    println!("\nDECISION WORKERS");
    println!("----------------");

    if configs.is_empty() {
        println!("No Decision worker configurations found.\n");
        return;
    }

    let mut up_count = 0;
    let mut missing_models = HashMap::new();

    for (host, port, expected_model) in configs {
        let endpoint_key = format!("{}:{}", host, port);

        match results.get(&endpoint_key) {
            Some(EndpointStatus::Up(available_models)) => {
                up_count += 1;
                println!("✅ {}:{} - UP", host, port);

                // Check if the expected model is available
                if available_models.contains(expected_model) {
                    println!("  ✅ AVAILABLE: {}", expected_model);
                } else {
                    println!("  ❌ MISSING: {}", expected_model);

                    // Track missing models
                    missing_models
                        .entry(expected_model.clone())
                        .or_insert_with(Vec::new)
                        .push(endpoint_key.clone());
                }

                // Show additional models available
                let additional_models: Vec<&String> = available_models
                    .iter()
                    .filter(|m| *m != expected_model)
                    .collect();

                if !additional_models.is_empty() {
                    println!("  ℹ️ ADDITIONAL MODELS:");
                    for model in additional_models {
                        println!("    - {}", model);
                    }
                }
            }
            Some(EndpointStatus::Down(error)) => {
                println!("❌ {}:{} - DOWN", host, port);
                println!("  ⚠️ Error: {}", error);

                // Track missing model
                missing_models
                    .entry(expected_model.clone())
                    .or_insert_with(Vec::new)
                    .push(endpoint_key);
            }
            None => {
                println!("❓ {}:{} - UNKNOWN (not tested)", host, port);
            }
        }
        println!();
    }

    println!("📋 Summary: {}/{} endpoints UP", up_count, configs.len());

    if !configs.is_empty() {
        let percentage = (up_count as f64 / configs.len() as f64) * 100.0;
        println!("   {:.1}% availability", percentage);
    }
}

/// Print report for Analysis worker endpoints
fn print_analysis_report(
    configs: &[(String, u16, String, Option<(String, u16, String)>)],
    results: &HashMap<String, EndpointStatus>,
) {
    println!("\nANALYSIS WORKERS");
    println!("----------------");

    if configs.is_empty() {
        println!("No Analysis worker configurations found.\n");
        return;
    }

    let mut main_endpoints = 0;
    let mut main_up_count = 0;
    let mut fallback_endpoints = 0;
    let mut fallback_up_count = 0;
    let mut missing_models = HashMap::new();

    for (host, port, expected_model, fallback) in configs {
        let endpoint_key = format!("{}:{}", host, port);
        main_endpoints += 1;

        match results.get(&endpoint_key) {
            Some(EndpointStatus::Up(available_models)) => {
                main_up_count += 1;
                println!("✅ {}:{} - UP", host, port);

                // Check if the expected model is available
                if available_models.contains(expected_model) {
                    println!("  ✅ AVAILABLE: {}", expected_model);
                } else {
                    println!("  ❌ MISSING: {}", expected_model);

                    // Track missing models
                    missing_models
                        .entry(expected_model.clone())
                        .or_insert_with(Vec::new)
                        .push(endpoint_key.clone());
                }

                // Show additional models
                let additional_models: Vec<&String> = available_models
                    .iter()
                    .filter(|m| *m != expected_model)
                    .collect();

                if !additional_models.is_empty() {
                    println!("  ℹ️ ADDITIONAL MODELS:");
                    for model in additional_models {
                        println!("    - {}", model);
                    }
                }
            }
            Some(EndpointStatus::Down(error)) => {
                println!("❌ {}:{} - DOWN", host, port);
                println!("  ⚠️ Error: {}", error);

                // Track missing model
                missing_models
                    .entry(expected_model.clone())
                    .or_insert_with(Vec::new)
                    .push(endpoint_key);
            }
            None => {
                println!("❓ {}:{} - UNKNOWN (not tested)", host, port);
            }
        }

        // Check fallback endpoint if present
        if let Some((fallback_host, fallback_port, fallback_model)) = fallback {
            fallback_endpoints += 1;
            let fallback_key = format!("{}:{} (fallback)", fallback_host, fallback_port);

            match results.get(&fallback_key) {
                Some(EndpointStatus::Up(available_models)) => {
                    fallback_up_count += 1;
                    println!("  ✅ FALLBACK {}:{} - UP", fallback_host, fallback_port);

                    // Check if the fallback model is available
                    if available_models.contains(fallback_model) {
                        println!("    ✅ AVAILABLE: {}", fallback_model);
                    } else {
                        println!("    ❌ MISSING: {}", fallback_model);

                        // Track missing models
                        missing_models
                            .entry(fallback_model.clone())
                            .or_insert_with(Vec::new)
                            .push(fallback_key.clone());
                    }
                }
                Some(EndpointStatus::Down(error)) => {
                    println!("  ❌ FALLBACK {}:{} - DOWN", fallback_host, fallback_port);
                    println!("    ⚠️ Error: {}", error);

                    // Track missing model
                    missing_models
                        .entry(fallback_model.clone())
                        .or_insert_with(Vec::new)
                        .push(fallback_key);
                }
                None => {
                    println!(
                        "  ❓ FALLBACK {}:{} - UNKNOWN (not tested)",
                        fallback_host, fallback_port
                    );
                }
            }
        }

        println!();
    }

    println!("📋 Summary:");
    println!("   Main endpoints: {}/{} UP", main_up_count, main_endpoints);
    if main_endpoints > 0 {
        let percentage = (main_up_count as f64 / main_endpoints as f64) * 100.0;
        println!("   {:.1}% main availability", percentage);
    }

    if fallback_endpoints > 0 {
        println!(
            "   Fallback endpoints: {}/{} UP",
            fallback_up_count, fallback_endpoints
        );
        let percentage = (fallback_up_count as f64 / fallback_endpoints as f64) * 100.0;
        println!("   {:.1}% fallback availability", percentage);
    }
}

/// Print overall summary
fn print_summary(
    decision_configs: &[(String, u16, String)],
    decision_results: &HashMap<String, EndpointStatus>,
    analysis_configs: &[(String, u16, String, Option<(String, u16, String)>)],
    analysis_results: &HashMap<String, EndpointStatus>,
) {
    println!("\nOVERALL SUMMARY");
    println!("--------------");

    // Gather missing models across all endpoints
    let mut all_missing_models: HashMap<String, Vec<String>> = HashMap::new();

    // Check Decision endpoints for missing models
    for (host, port, model) in decision_configs {
        let endpoint_key = format!("{}:{}", host, port);
        if let Some(EndpointStatus::Up(available_models)) = decision_results.get(&endpoint_key) {
            if !available_models.contains(model) {
                all_missing_models
                    .entry(model.clone())
                    .or_insert_with(Vec::new)
                    .push(endpoint_key.clone());
            }
        } else {
            // Endpoint down or not tested
            all_missing_models
                .entry(model.clone())
                .or_insert_with(Vec::new)
                .push(endpoint_key.clone());
        }
    }

    // Check Analysis endpoints for missing models
    for (host, port, model, fallback) in analysis_configs {
        let endpoint_key = format!("{}:{}", host, port);

        // Check main endpoint
        if let Some(EndpointStatus::Up(available_models)) = analysis_results.get(&endpoint_key) {
            if !available_models.contains(model) {
                all_missing_models
                    .entry(model.clone())
                    .or_insert_with(Vec::new)
                    .push(endpoint_key.clone());
            }
        } else {
            // Endpoint down or not tested
            all_missing_models
                .entry(model.clone())
                .or_insert_with(Vec::new)
                .push(endpoint_key.clone());
        }

        // Check fallback endpoint if present
        if let Some((fallback_host, fallback_port, fallback_model)) = fallback {
            let fallback_key = format!("{}:{} (fallback)", fallback_host, fallback_port);
            if let Some(EndpointStatus::Up(available_models)) = analysis_results.get(&fallback_key)
            {
                if !available_models.contains(fallback_model) {
                    all_missing_models
                        .entry(fallback_model.clone())
                        .or_insert_with(Vec::new)
                        .push(fallback_key.clone());
                }
            } else {
                // Fallback endpoint down or not tested
                all_missing_models
                    .entry(fallback_model.clone())
                    .or_insert_with(Vec::new)
                    .push(fallback_key.clone());
            }
        }
    }

    // Count up endpoints
    let decision_up = decision_results
        .values()
        .filter(|s| matches!(s, EndpointStatus::Up(_)))
        .count();

    let analysis_up = analysis_results
        .values()
        .filter(|s| matches!(s, EndpointStatus::Up(_)))
        .count();

    let total_configs = decision_configs.len()
        + analysis_configs.len()
        + analysis_configs
            .iter()
            .filter_map(|(_, _, _, f)| f.as_ref())
            .count();
    let total_up = decision_up + analysis_up;

    // Print summary statistics
    println!(
        "Total endpoints: {}/{} UP ({:.1}%)",
        total_up,
        total_configs,
        if total_configs > 0 {
            (total_up as f64 / total_configs as f64) * 100.0
        } else {
            0.0
        }
    );

    // Print missing models
    if all_missing_models.is_empty() {
        println!("\n🎉 All configured models are available on their respective endpoints!");
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

    // Get the environment variables
    let decision_ollama_config_str = env::var(DECISION_OLLAMA_CONFIGS_ENV).unwrap_or_default();
    let analysis_ollama_config_str = env::var(ANALYSIS_OLLAMA_CONFIGS_ENV).unwrap_or_default();

    // Parse configurations using the shared functions
    let decision_configs = process_ollama_configs(&decision_ollama_config_str);
    let analysis_configs = process_analysis_ollama_configs(&analysis_ollama_config_str);

    info!(
        "Testing {} decision endpoints and {} analysis endpoints",
        decision_configs.len(),
        analysis_configs.len()
    );

    if decision_configs.is_empty() && analysis_configs.is_empty() {
        println!("\n⚠️ No Ollama configurations found in environment variables.");
        println!("Please set DECISION_OLLAMA_CONFIGS and/or ANALYSIS_OLLAMA_CONFIGS");
        println!("Format: host|port|model;host|port|model;...");
        println!("Analysis format (with optional fallback): host|port|model||fallback_host|fallback_port|fallback_model;...");
        return Ok(());
    }

    // Test all endpoints
    let decision_results = test_decision_endpoints(&decision_configs).await;
    let analysis_results = test_analysis_endpoints(&analysis_configs).await;

    // Generate and print reports
    print_decision_report(&decision_configs, &decision_results);
    print_analysis_report(&analysis_configs, &analysis_results);

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
