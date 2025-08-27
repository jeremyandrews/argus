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

use argus::analysis_worker;
use argus::app::api;
use argus::decision_worker;
use argus::environment;
use argus::logging;
use argus::rate_limiter::OpenAIRateLimiter;
use argus::rss;
use argus::{
    FallbackConfig, LLMClient, ModelConfig, START_TIME, TARGET_LLM_REQUEST, TARGET_WEB_REQUEST,
};

use environment::get_env_var_as_vec;

// New: Struct to hold Analysis Worker configuration including optional fallback
#[derive(Clone, Debug)]
struct AnalysisWorkerConfig {
    id: i16,
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
                id: *count,
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
                id: *count,
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

    // Determine number of decision workers to launch
    // Use the shared parse functions to determine the count
    let decision_worker_count = argus::process_ollama_configs(&decision_ollama_configs).len()
        + decision_openai_configs
            .split(';')
            .filter(|c| !c.is_empty())
            .count();

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

    // Launch DECISION workers
    let mut decision_handles = Vec::new();
    for (decision_id, llm_client, decision_model, no_think) in
        decision_workers.into_iter().take(decision_worker_count)
    {
        let decision_worker_topics = topics.clone();
        let decision_worker_slack_token = slack_token.clone();
        let decision_worker_slack_channel = slack_channel.clone();
        let worker_notify = Arc::clone(&panic_notify);
        let thread_name = format!("Decision Worker {}", decision_id);
        let worker_rate_limiter = shared_openai_rate_limiter.clone();

        let decision_worker_handle = tokio::spawn(async move {
            info!(target: TARGET_LLM_REQUEST, "{}: Starting Decision Worker with model '{}' (decision_loop)", thread_name, decision_model);

            // Create model config based on thinking vs non-thinking mode with environment overrides
            let worker_model_config = Some(create_model_config(
                no_think, env_top_p, env_top_k, env_min_p,
            ));
            let worker_temperature = get_temperature(no_think, env_temperature);

            info!(
                target: TARGET_LLM_REQUEST,
                "{}: Using {} mode with temp={}, top_p={}, top_k={}, min_p={}",
                thread_name,
                if no_think { "non-thinking" } else { "thinking" },
                worker_temperature,
                worker_model_config.as_ref().unwrap().top_p,
                worker_model_config.as_ref().unwrap().top_k,
                worker_model_config.as_ref().unwrap().min_p
            );

            match decision_worker::decision_loop(
                decision_id,
                &decision_worker_topics,
                &llm_client,
                &decision_model,
                worker_temperature,
                &decision_worker_slack_token,
                &decision_worker_slack_channel,
                no_think,
                worker_model_config,
                worker_rate_limiter,
            )
            .await
            {
                Ok(_) => {
                    info!(target: TARGET_LLM_REQUEST, "{}: decision_loop completed successfully.", thread_name)
                }
                Err(e) => {
                    error!(target: TARGET_LLM_REQUEST, "{}: decision_loop failed: {}", thread_name, e);
                    worker_notify.notify_one();
                }
            }
        });
        decision_handles.push(decision_worker_handle);
    }

    // Launch ANALYSIS workers with optional fallback
    let mut analysis_handles = Vec::new();
    for worker_config in analysis_workers.into_iter() {
        let decision_worker_topics = topics.clone();
        let analysis_worker_slack_token = slack_token.clone();
        let analysis_worker_slack_channel = slack_channel.clone();
        let worker_notify = Arc::clone(&panic_notify);
        let thread_name = format!("Analysis Worker {}", worker_config.id);

        // Use the worker's configured model
        let worker_model = worker_config.model.clone();

        let analysis_handle = tokio::spawn(async move {
            info!(target: TARGET_LLM_REQUEST, "{}: Starting Analysis Worker with model '{}' (analysis_loop)", thread_name, worker_model);

            // Create model config based on thinking vs non-thinking mode with environment overrides
            let worker_model_config = Some(create_model_config(
                worker_config.no_think,
                env_top_p,
                env_top_k,
                env_min_p,
            ));
            let worker_temperature = get_temperature(worker_config.no_think, env_temperature);

            info!(
                target: TARGET_LLM_REQUEST,
                "{}: Using {} mode with temp={}, top_p={}, top_k={}, min_p={}",
                thread_name,
                if worker_config.no_think { "non-thinking" } else { "thinking" },
                worker_temperature,
                worker_model_config.as_ref().unwrap().top_p,
                worker_model_config.as_ref().unwrap().top_k,
                worker_model_config.as_ref().unwrap().min_p
            );

            match analysis_worker::analysis_loop(
                worker_config.id,
                &decision_worker_topics,
                &worker_config.llm_client,
                &worker_model,
                &analysis_worker_slack_token,
                &analysis_worker_slack_channel,
                worker_temperature,
                worker_config.fallback,
                worker_model_config,
                worker_config.no_think,
            )
            .await
            {
                Ok(_) => {
                    info!(target: TARGET_LLM_REQUEST, "{}: analysis_loop completed successfully.", thread_name)
                }
                Err(e) => {
                    error!(target: TARGET_LLM_REQUEST, "{}: analysis_loop failed: {}", thread_name, e);
                    worker_notify.notify_one();
                }
            }
        });
        analysis_handles.push(analysis_handle);
    }

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

    Ok(())
}
