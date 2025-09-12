use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use futures::future::join_all;
use ollama_rs::Ollama;
use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tracing::{error, info, warn};

const DECISION_OLLAMA_CONFIGS_ENV: &str = "DECISION_OLLAMA_CONFIGS";
const ANALYSIS_OLLAMA_CONFIGS_ENV: &str = "ANALYSIS_OLLAMA_CONFIGS";
const DECISION_OPENAI_CONFIGS_ENV: &str = "DECISION_OPENAI_CONFIGS";
const ANALYSIS_OPENAI_CONFIGS_ENV: &str = "ANALYSIS_OPENAI_CONFIGS";
const SLACK_TOKEN_ENV: &str = "SLACK_TOKEN";
const SLACK_CHANNEL_ENV: &str = "SLACK_CHANNEL";
const LLM_TEMPERATURE_ENV: &str = "LLM_TEMPERATURE";
const LLM_TOP_P_ENV: &str = "LLM_TOP_P";
const LLM_TOP_K_ENV: &str = "LLM_TOP_K";
const LLM_MIN_P_ENV: &str = "LLM_MIN_P";

use argus::app::api;
use argus::environment;
use argus::logging;
use argus::rate_limiter::OpenAIRateLimiter;
use argus::rss;
use argus::workers;
use argus::{
    FallbackConfig, LLMClient, ModelConfig, START_TIME, TARGET_LLM_REQUEST, TARGET_WEB_REQUEST,
};

use environment::get_env_var_as_vec;

// New: Struct to hold Analysis Worker configuration including optional fallback
#[derive(Clone, Debug)]
struct AnalysisWorkerConfig {
    llm_client: LLMClient,
    model: String,
    fallback: Option<FallbackConfig>,
    no_think: bool,
}

pub fn initialize_start_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    START_TIME.store(now, Ordering::SeqCst);
}

/// Create ModelConfig based on thinking vs non-thinking mode with environment overrides
fn create_model_config(
    no_think: bool,
    env_top_p: f32,
    env_top_k: i32,
    env_min_p: f32,
) -> ModelConfig {
    if no_think {
        // Non-thinking mode parameters
        ModelConfig {
            strip_thinking_tags: true,
            top_p: if env_top_p > 0.0 { env_top_p } else { 0.8 },
            top_k: if env_top_k > 0 { env_top_k } else { 20 },
            min_p: if env_min_p >= 0.0 { env_min_p } else { 0.0 },
        }
    } else {
        // Thinking mode parameters
        ModelConfig {
            strip_thinking_tags: true,
            top_p: if env_top_p > 0.0 { env_top_p } else { 0.95 },
            top_k: if env_top_k > 0 { env_top_k } else { 20 },
            min_p: if env_min_p >= 0.0 { env_min_p } else { 0.0 },
        }
    }
}

/// Calculate temperature based on thinking mode and environment overrides
/// Note: GPT-5 temperature restrictions are now handled centrally in llm.rs
fn get_temperature(no_think: bool, env_temperature: f32) -> f32 {
    if no_think {
        // Non-thinking mode: use 0.7 or environment override
        if env_temperature > 0.0 {
            env_temperature
        } else {
            0.7
        }
    } else {
        // Thinking mode: use 0.6 or environment override
        if env_temperature > 0.0 {
            env_temperature
        } else {
            0.6
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    initialize_start_time();
    logging::configure_logging();

    // Read the DECISION and ANALYSIS environment variables
    let decision_ollama_configs = env::var(DECISION_OLLAMA_CONFIGS_ENV).unwrap_or_default();
    let analysis_ollama_configs = env::var(ANALYSIS_OLLAMA_CONFIGS_ENV).unwrap_or_default();
    let decision_openai_configs = env::var(DECISION_OPENAI_CONFIGS_ENV).unwrap_or_default();
    let analysis_openai_configs = env::var(ANALYSIS_OPENAI_CONFIGS_ENV).unwrap_or_default();

    let mut decision_workers = Vec::new();
    let mut decision_count: i16 = 0;

    // Change: Use AnalysisWorkerConfig for analysis_workers
    let mut analysis_workers: Vec<AnalysisWorkerConfig> = Vec::new();
    let mut analysis_count: i16 = 0;

    // Process Ollama and OpenAI configs using shared functions
    fn process_ollama_configs_for_workers(
        configs: &str,
        workers: &mut Vec<(i16, LLMClient, String, bool)>,
        count: &mut i16,
    ) {
        for (host, port, model, no_think) in argus::process_ollama_configs(configs) {
            info!(
                "Configuring Ollama worker {} to connect to model '{}' at {}:{} (no_think: {})",
                *count, model, host, port, no_think
            );
            workers.push((
                *count,
                LLMClient::Ollama(Ollama::new(host, port)),
                model,
                no_think,
            ));
            *count += 1;
        }
    }

    fn process_openai_configs(
        configs: &str,
        workers: &mut Vec<(i16, LLMClient, String, bool)>,
        count: &mut i16,
    ) {
        for config in configs.split(';').filter(|c| !c.is_empty()) {
            let parts: Vec<&str> = config.split('|').collect();
            if parts.len() != 2 {
                error!("Invalid OpenAI configuration format: {}", config);
                continue;
            }
            let api_key = parts[0].to_string();
            let model = parts[1].to_string();
            let config = OpenAIConfig::new().with_api_key(&api_key);
            let client = OpenAIClient::with_config(config);
            info!(
                "Configuring OpenAI worker {} to connect to model '{}'",
                *count, model
            );
            // OpenAI doesn't support no_think mode
            workers.push((*count, LLMClient::OpenAI(client), model, false));
            *count += 1;
        }
    }

    // Process Analysis config using shared functions
    fn process_analysis_ollama_configs_for_workers(
        configs: &str,
        workers: &mut Vec<AnalysisWorkerConfig>,
        count: &mut i16,
    ) {
        for (host, port, model, no_think, fallback) in
            argus::process_analysis_ollama_configs(configs)
        {
            // Create main LLM client
            let main_llm_client = LLMClient::Ollama(Ollama::new(host.clone(), port));

            // Create fallback config if present
            let fallback_config = fallback.map(
                |(fallback_host, fallback_port, fallback_model, fallback_no_think)| {
                    FallbackConfig {
                        llm_client: LLMClient::Ollama(Ollama::new(
                            fallback_host.clone(),
                            fallback_port,
                        )),
                        model: fallback_model,
                        no_think: fallback_no_think,
                    }
                },
            );

            info!(
                "Configuring Analysis worker {} to connect to model '{}' at {}:{} (no_think: {})",
                *count, model, host, port, no_think
            );

            workers.push(AnalysisWorkerConfig {
                llm_client: main_llm_client,
                model,
                fallback: fallback_config,
                no_think,
            });
            *count += 1;
        }
    }

    // Process Analysis OpenAI configurations
    fn process_analysis_openai_configs(
        configs: &str,
        workers: &mut Vec<AnalysisWorkerConfig>,
        count: &mut i16,
    ) {
        for config in configs.split(';').filter(|c| !c.is_empty()) {
            // Split main and fallback configurations
            let parts: Vec<&str> = config.split("||").collect();
            if parts.is_empty() {
                error!("Invalid Analysis OpenAI configuration format: {}", config);
                continue;
            }

            // Process main configuration
            let main_parts: Vec<&str> = parts[0].split('|').collect();
            if main_parts.len() != 2 {
                error!(
                    "Invalid main OpenAI configuration format for Analysis worker: {}",
                    parts[0]
                );
                continue;
            }
            let main_api_key = main_parts[0].to_string();
            let main_model = main_parts[1].to_string();
            let main_config = OpenAIConfig::new().with_api_key(&main_api_key);
            let main_client = OpenAIClient::with_config(main_config);
            let main_llm_client = LLMClient::OpenAI(main_client);

            // Process fallback configuration if present
            let fallback = if parts.len() > 1 {
                let fallback_parts: Vec<&str> = parts[1].split('|').collect();
                if fallback_parts.len() != 2 {
                    error!(
                        "Invalid fallback OpenAI configuration format for Analysis worker: {}",
                        parts[1]
                    );
                    None
                } else {
                    let fallback_api_key = fallback_parts[0].to_string();
                    let fallback_model = fallback_parts[1].to_string();
                    let fallback_config = OpenAIConfig::new().with_api_key(&fallback_api_key);
                    Some(FallbackConfig {
                        llm_client: LLMClient::OpenAI(OpenAIClient::with_config(fallback_config)),
                        model: fallback_model,
                        no_think: false, // OpenAI doesn't support no_think mode
                    })
                }
            } else {
                None
            };

            info!(
                "Configuring Analysis worker {} to connect to model '{}' with API key.",
                *count, main_model
            );
            workers.push(AnalysisWorkerConfig {
                llm_client: main_llm_client,
                model: main_model,
                fallback,
                no_think: false, // OpenAI doesn't support no_think mode
            });
            *count += 1;
        }
    }

    // Process DECISION configurations
    process_ollama_configs_for_workers(
        &decision_ollama_configs,
        &mut decision_workers,
        &mut decision_count,
    );
    process_openai_configs(
        &decision_openai_configs,
        &mut decision_workers,
        &mut decision_count,
    );

    // Log DECISION workers
    info!(
        "Total decision workers configured: {}",
        decision_workers.len()
    );

    // Load ANALYSIS configurations with possible fallback
    process_analysis_ollama_configs_for_workers(
        &analysis_ollama_configs,
        &mut analysis_workers,
        &mut analysis_count,
    );
    process_analysis_openai_configs(
        &analysis_openai_configs,
        &mut analysis_workers,
        &mut analysis_count,
    );

    // Log ANALYSIS workers
    info!(
        "Total analysis workers configured: {}",
        analysis_workers.len()
    );

    let urls = get_env_var_as_vec("URLS", ';');
    let topics = get_env_var_as_vec("TOPICS", ';');
    let slack_token = env::var(SLACK_TOKEN_ENV).expect("SLACK_TOKEN environment variable required");
    let slack_channel =
        env::var(SLACK_CHANNEL_ENV).expect("SLACK_CHANNEL environment variable required");

    // Read environment parameter overrides (optional)
    let env_temperature = env::var(LLM_TEMPERATURE_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0); // 0.0 means use automatic values

    let env_top_p = env::var(LLM_TOP_P_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0); // 0.0 means use automatic values

    let env_top_k = env::var(LLM_TOP_K_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0); // 0 means use automatic values

    let env_min_p = env::var(LLM_MIN_P_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1.0); // -1.0 means use automatic values (since 0.0 is a valid value)

    info!(
        "Environment parameter overrides: temp={} (0.0=auto), top_p={} (0.0=auto), top_k={} (0=auto), min_p={} (-1.0=auto)",
        env_temperature, env_top_p, env_top_k, env_min_p
    );

    // Create shared OpenAI rate limiter for all workers
    let shared_openai_rate_limiter = match OpenAIRateLimiter::from_env() {
        Ok(limiter) => {
            info!(target: TARGET_LLM_REQUEST, "Created shared OpenAI rate limiter for all workers");
            Some(Arc::new(limiter))
        }
        Err(e) => {
            warn!(target: TARGET_LLM_REQUEST, "Failed to create shared rate limiter: {}. OpenAI requests will not be rate limited.", e);
            None
        }
    };

    // Define panic notification mechanism
    let panic_notify = Arc::new(Notify::new());

    // Spawn the app_api_loop in a new thread
    let app_api_notify = Arc::clone(&panic_notify);
    let app_api_handle = tokio::spawn(async move {
        let thread_name = "App API Loop".to_string();
        info!("{}: Starting App API (app_api_loop)", thread_name);
        match api::app_api_loop().await {
            Ok(_) => {
                info!(target: TARGET_WEB_REQUEST, "{}: app_api_loop completed successfully.", thread_name)
            }
            Err(e) => {
                error!("{}: app_api_loop failed: {}", thread_name, e);
                app_api_notify.notify_one();
            }
        }
    });

    // Spawn a thread to parse URLs from RSS feeds with monitoring.
    let rss_notify = Arc::clone(&panic_notify);
    let rss_handle = tokio::spawn(async move {
        let thread_name = "RSS Feed Parser".to_string();
        info!(target: TARGET_WEB_REQUEST, "{}: Starting RSS feed parsing (rss_loop).", thread_name);
        match rss::rss_loop(urls.clone()).await {
            Ok(_) => {
                info!(target: TARGET_WEB_REQUEST, "{}: rss_loop completed successfully.", thread_name)
            }
            Err(e) => {
                error!(target: TARGET_WEB_REQUEST, "{}: rss_loop failed: {}", thread_name, e);
                rss_notify.notify_one();
            }
        }
    });

    // Spawn the maintenance worker
    let maintenance_notify = Arc::clone(&panic_notify);
    let maintenance_handle = tokio::spawn(async move {
        let thread_name = "Maintenance Worker".to_string();
        info!("{}: Starting maintenance worker", thread_name);
        match workers::maintenance::run_maintenance_worker().await {
            Ok(_) => {
                info!("{}: maintenance worker completed successfully", thread_name)
            }
            Err(e) => {
                error!("{}: maintenance worker failed: {}", thread_name, e);
                maintenance_notify.notify_one();
            }
        }
    });

    // Initialize Dynamic Pool Manager for intelligent cost optimization
    info!("[MODEL_SCALING] Initializing Dynamic Pool Manager with intelligent cost optimization");
    info!("[MODEL_SCALING] Converting configurations to PoolManager format");

    // Convert existing configurations to WorkerConfig format for PoolManager
    let mut decision_ollama_worker_configs = Vec::new();
    let mut decision_openai_worker_configs = Vec::new();
    let mut analysis_ollama_worker_configs = Vec::new();
    let mut analysis_openai_worker_configs = Vec::new();

    // Process Decision workers
    for (_, llm_client, model, no_think) in decision_workers {
        let temperature = get_temperature(no_think, env_temperature);
        let model_config = Some(create_model_config(
            no_think, env_top_p, env_top_k, env_min_p,
        ));

        let worker_config = workers::pool_manager::WorkerConfig {
            worker_type: workers::pool_manager::WorkerType::Decision,
            llm_client,
            model,
            no_think,
            fallback: None,
            temperature,
            model_config,
            shared_rate_limiter: shared_openai_rate_limiter.clone(),
        };

        match worker_config.llm_client {
            LLMClient::Ollama(_) => decision_ollama_worker_configs.push(worker_config),
            LLMClient::OpenAI(_) => decision_openai_worker_configs.push(worker_config),
        }
    }

    // Process Analysis workers
    for worker_config in analysis_workers {
        let temperature = get_temperature(worker_config.no_think, env_temperature);
        let model_config = Some(create_model_config(
            worker_config.no_think,
            env_top_p,
            env_top_k,
            env_min_p,
        ));

        let pool_worker_config = workers::pool_manager::WorkerConfig {
            worker_type: workers::pool_manager::WorkerType::Analysis,
            llm_client: worker_config.llm_client,
            model: worker_config.model,
            no_think: worker_config.no_think,
            fallback: worker_config.fallback,
            temperature,
            model_config,
            shared_rate_limiter: shared_openai_rate_limiter.clone(),
        };

        match pool_worker_config.llm_client {
            LLMClient::Ollama(_) => analysis_ollama_worker_configs.push(pool_worker_config),
            LLMClient::OpenAI(_) => analysis_openai_worker_configs.push(pool_worker_config),
        }
    }

    info!(
        "[MODEL_SCALING] Free Ollama models: {} Decision + {} Analysis - will run continuously",
        decision_ollama_worker_configs.len(),
        analysis_ollama_worker_configs.len()
    );
    info!("[MODEL_SCALING] Paid OpenAI models: {} Decision + {} Analysis - will scale based on queue pressure", 
          decision_openai_worker_configs.len(), analysis_openai_worker_configs.len());

    // Create database connection for PoolManager using proper db/ module abstraction
    let db = argus::db::core::Database::new_for_pool_manager().await?;

    // Create scaling configuration
    let scaling_config = workers::pool_manager::ScalingConfig::default(); // Uses scale_up: 10, scale_down: 2

    // Initialize PoolManager
    let pool_manager = workers::pool_manager::PoolManager::new(
        decision_ollama_worker_configs,
        analysis_ollama_worker_configs,
        decision_openai_worker_configs,
        analysis_openai_worker_configs,
        scaling_config,
        db,
        topics,
        slack_token,
        slack_channel,
    );

    // Start the pool manager (launches Ollama workers immediately, manages OpenAI workers dynamically)
    info!("[MODEL_SCALING] Starting PoolManager with intelligent cost optimization");
    if let Err(e) = pool_manager.start().await {
        error!("[MODEL_SCALING] Failed to start PoolManager: {}", e);
        return Err(e);
    }

    info!("[MODEL_SCALING] Dynamic pool management active - queue pressure monitoring enabled");
    info!("[MODEL_SCALING] Scale up threshold: >10 articles, Scale down threshold: <2 articles");
    info!("[MODEL_SCALING] Decision workers monitor RSS queue, Analysis workers monitor Life Safety + Matched Topics queues");

    // Keep the pool manager running - it manages its own worker lifecycle
    let pool_manager_handle = tokio::spawn(async move {
        // PoolManager runs continuously managing workers
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl+c");
        info!("[MODEL_SCALING] Shutting down pool manager");
        pool_manager.shutdown().await;
    });

    let decision_handles = vec![pool_manager_handle];
    let analysis_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // Spawn a watcher for any thread failures
    let panic_notify_clone = Arc::clone(&panic_notify);
    let watcher_handle = tokio::spawn(async move {
        panic_notify_clone.notified().await;
        error!("A thread has exited or panicked. Triggering main process panic.");
        panic!("Thread failure detected");
    });

    let decision_results = join_all(decision_handles).await;
    for (i, result) in decision_results.into_iter().enumerate() {
        if let Err(e) = result {
            error!(target: TARGET_LLM_REQUEST, "Decision worker {} failed: {}", i, e);
        }
    }

    let analysis_results = join_all(analysis_handles).await;
    for (i, result) in analysis_results.into_iter().enumerate() {
        if let Err(e) = result {
            error!(target: TARGET_LLM_REQUEST, "Analysis worker {} failed: {}", i, e);
        }
    }

    watcher_handle.await.ok();

    // Await app_api completion
    if let Err(e) = app_api_handle.await {
        error!(target: TARGET_WEB_REQUEST, "App API (app_api_loop) encountered an error: {}", e);
    }

    // Await rss_loop completion
    if let Err(e) = rss_handle.await {
        error!(target: TARGET_WEB_REQUEST, "RSS task (rss_loop) encountered an error: {}", e);
    }

    // Await maintenance worker completion
    if let Err(e) = maintenance_handle.await {
        error!("Maintenance worker encountered an error: {}", e);
    }

    Ok(())
}
