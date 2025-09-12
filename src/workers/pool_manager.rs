//! Dynamic Worker Pool Management System
//!
//! This module provides intelligent cost optimization by dynamically scaling between
//! free local Ollama models and paid OpenAI models based on queue pressure.
//!
//! Key features:
//! - Independent scaling for Decision vs Analysis workers based on relevant queue pressure
//! - NEVER scales down free Ollama models (they run permanently)
//! - Only scales OpenAI models up/down based on cost optimization
//! - Comprehensive [MODEL_SCALING] tagged logging for operational monitoring

use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use ollama_rs::Ollama;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::db::core::Database;
use crate::rate_limiter::OpenAIRateLimiter;
use crate::{analysis_worker, decision_worker, FallbackConfig, LLMClient, ModelConfig};

/// Configuration for scaling behavior
#[derive(Clone, Debug)]
pub struct ScalingConfig {
    /// Threshold to start adding OpenAI workers (queue pressure)
    pub scale_up_threshold: u32,
    /// Threshold to start removing OpenAI workers (more aggressive to avoid falling behind)
    pub scale_down_threshold: u32,
    /// Maximum number of OpenAI workers per type
    pub max_openai_workers: u32,
    /// Interval between scaling decisions
    pub scaling_interval: Duration,
    /// Cooldown period after scaling operations
    pub scaling_cooldown: Duration,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            scale_up_threshold: 10,
            scale_down_threshold: 2, // More aggressive scale-down to avoid falling behind
            max_openai_workers: 5,
            scaling_interval: Duration::from_secs(30),
            scaling_cooldown: Duration::from_secs(120),
        }
    }
}

/// Handle to a running worker for lifecycle management
#[derive(Debug)]
pub struct WorkerHandle {
    pub id: String,
    pub worker_type: WorkerType,
    pub model_type: ModelType,
    pub handle: JoinHandle<()>,
    pub shutdown_notify: Arc<Notify>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerType {
    Decision,
    Analysis,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum ModelType {
    Ollama, // Free - never scaled down
    OpenAI, // Paid - scaled up/down based on pressure
}

/// Configuration for a worker
#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub worker_type: WorkerType,
    pub llm_client: LLMClient,
    pub model: String,
    pub no_think: bool,
    pub fallback: Option<FallbackConfig>,
    pub temperature: f32,
    pub model_config: Option<ModelConfig>,
    pub shared_rate_limiter: Option<Arc<OpenAIRateLimiter>>,
}

/// Manages pools of decision and analysis workers independently
pub struct PoolManager {
    /// Active worker handles
    active_workers: Arc<Mutex<Vec<WorkerHandle>>>,
    /// Available Ollama worker configurations (NEVER scaled down - they're free)
    ollama_decision_configs: Vec<WorkerConfig>,
    ollama_analysis_configs: Vec<WorkerConfig>,
    /// Available OpenAI worker configurations (scaled up/down based on pressure)
    openai_decision_configs: Vec<WorkerConfig>,
    openai_analysis_configs: Vec<WorkerConfig>,
    /// Scaling configuration
    scaling_config: ScalingConfig,
    /// Database handle for queue monitoring
    database: Arc<Database>,
    /// Shared environment configuration
    topics: Vec<String>,
    slack_token: String,
    slack_channel: String,
    /// Last scaling operation time per worker type
    last_decision_scaling: Arc<Mutex<Option<Instant>>>,
    last_analysis_scaling: Arc<Mutex<Option<Instant>>>,
    /// Global shutdown notification
    shutdown_notify: Arc<Notify>,
}

impl PoolManager {
    pub fn new(
        ollama_decision_configs: Vec<WorkerConfig>,
        ollama_analysis_configs: Vec<WorkerConfig>,
        openai_decision_configs: Vec<WorkerConfig>,
        openai_analysis_configs: Vec<WorkerConfig>,
        scaling_config: ScalingConfig,
        database: Arc<Database>,
        topics: Vec<String>,
        slack_token: String,
        slack_channel: String,
    ) -> Self {
        Self {
            active_workers: Arc::new(Mutex::new(Vec::new())),
            ollama_decision_configs,
            ollama_analysis_configs,
            openai_decision_configs,
            openai_analysis_configs,
            scaling_config,
            database,
            topics,
            slack_token,
            slack_channel,
            last_decision_scaling: Arc::new(Mutex::new(None)),
            last_analysis_scaling: Arc::new(Mutex::new(None)),
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    /// Start the pool manager with initial Ollama workers
    pub async fn start(&self) -> Result<()> {
        info!("[MODEL_SCALING] Starting pool manager with {} Ollama Decision, {} Ollama Analysis, {} OpenAI Decision, {} OpenAI Analysis configs available", 
              self.ollama_decision_configs.len(), self.ollama_analysis_configs.len(),
              self.openai_decision_configs.len(), self.openai_analysis_configs.len());

        // Start all Ollama workers initially (these are NEVER scaled down - they're free)
        self.start_initial_workers().await?;

        // Start the scaling loop
        self.start_scaling_loop().await;

        Ok(())
    }

    /// Start initial Ollama workers only (never scaled down as they're free)
    async fn start_initial_workers(&self) -> Result<()> {
        info!("[MODEL_SCALING] Starting initial free Ollama workers");

        for config in &self.ollama_decision_configs {
            self.add_worker(config.clone()).await?;
        }

        for config in &self.ollama_analysis_configs {
            self.add_worker(config.clone()).await?;
        }

        Ok(())
    }

    /// Start the periodic scaling evaluation loop
    async fn start_scaling_loop(&self) {
        let self_clone = self.clone_for_loop().await;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self_clone.scaling_config.scaling_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = self_clone.evaluate_scaling().await {
                            error!("[MODEL_SCALING] Error during scaling evaluation: {}", e);
                        }
                    }
                    _ = self_clone.shutdown_notify.notified() => {
                        info!("[MODEL_SCALING] Scaling loop shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Clone necessary data for the async loop (avoiding full struct clone)
    async fn clone_for_loop(&self) -> PoolManagerLoop {
        PoolManagerLoop {
            active_workers: Arc::clone(&self.active_workers),
            openai_decision_configs: self.openai_decision_configs.clone(),
            openai_analysis_configs: self.openai_analysis_configs.clone(),
            scaling_config: self.scaling_config.clone(),
            database: Arc::clone(&self.database),
            topics: self.topics.clone(),
            slack_token: self.slack_token.clone(),
            slack_channel: self.slack_channel.clone(),
            last_decision_scaling: Arc::clone(&self.last_decision_scaling),
            last_analysis_scaling: Arc::clone(&self.last_analysis_scaling),
            shutdown_notify: Arc::clone(&self.shutdown_notify),
        }
    }

    /// Add a new worker to the pool
    async fn add_worker(&self, config: WorkerConfig) -> Result<()> {
        let worker_id = format!(
            "{:?}-{}-{}",
            config.worker_type,
            config.model,
            chrono::Utc::now().timestamp_millis()
        );

        let model_type = match config.llm_client {
            LLMClient::Ollama(_) => ModelType::Ollama,
            LLMClient::OpenAI(_) => ModelType::OpenAI,
        };

        let shutdown_notify = Arc::new(Notify::new());
        let worker_shutdown = Arc::clone(&shutdown_notify);

        let topics = self.topics.clone();
        let slack_token = self.slack_token.clone();
        let slack_channel = self.slack_channel.clone();

        let handle = match config.worker_type {
            WorkerType::Decision => {
                let config_clone = config.clone();
                let worker_id_clone = worker_id.clone();
                tokio::spawn(async move {
                    let thread_name = format!("Dynamic Decision Worker {}", worker_id_clone);
                    info!("[MODEL_SCALING] [WORKER_ADD] Starting {}", thread_name);

                    tokio::select! {
                        result = decision_worker::decision_loop(
                            0, // Dynamic workers use ID 0
                            &topics,
                            &config_clone.llm_client,
                            &config_clone.model,
                            config_clone.temperature,
                            &slack_token,
                            &slack_channel,
                            config_clone.no_think,
                            config_clone.model_config,
                            config_clone.shared_rate_limiter,
                        ) => {
                            match result {
                                Ok(_) => info!("[MODEL_SCALING] [WORKER_REMOVE] {} completed successfully", thread_name),
                                Err(e) => error!("[MODEL_SCALING] [WORKER_REMOVE] {} failed: {}", thread_name, e),
                            }
                        }
                        _ = worker_shutdown.notified() => {
                            info!("[MODEL_SCALING] [WORKER_REMOVE] {} shutting down on request", thread_name);
                        }
                    }
                })
            }
            WorkerType::Analysis => {
                let config_clone = config.clone();
                let worker_id_clone = worker_id.clone();
                tokio::spawn(async move {
                    let thread_name = format!("Dynamic Analysis Worker {}", worker_id_clone);
                    info!("[MODEL_SCALING] [WORKER_ADD] Starting {}", thread_name);

                    tokio::select! {
                        result = analysis_worker::analysis_loop(
                            0, // Dynamic workers use ID 0
                            &topics,
                            &config_clone.llm_client,
                            &config_clone.model,
                            &slack_token,
                            &slack_channel,
                            config_clone.temperature,
                            config_clone.fallback,
                            config_clone.model_config,
                            config_clone.no_think,
                        ) => {
                            match result {
                                Ok(_) => info!("[MODEL_SCALING] [WORKER_REMOVE] {} completed successfully", thread_name),
                                Err(e) => error!("[MODEL_SCALING] [WORKER_REMOVE] {} failed: {}", thread_name, e),
                            }
                        }
                        _ = worker_shutdown.notified() => {
                            info!("[MODEL_SCALING] [WORKER_REMOVE] {} shutting down on request", thread_name);
                        }
                    }
                })
            }
        };

        let worker = WorkerHandle {
            id: worker_id.clone(),
            worker_type: config.worker_type,
            model_type,
            handle,
            shutdown_notify,
        };

        let mut workers = self.active_workers.lock().await;
        workers.push(worker);

        info!(
            "[MODEL_SCALING] [WORKER_ADD] Added {} {} worker: {}",
            if model_type == ModelType::OpenAI {
                "OpenAI"
            } else {
                "Ollama"
            },
            if config.worker_type == WorkerType::Decision {
                "Decision"
            } else {
                "Analysis"
            },
            worker_id
        );

        Ok(())
    }

    /// Remove a worker from the pool
    async fn remove_worker(&self, worker: WorkerHandle) {
        info!(
            "[MODEL_SCALING] [WORKER_REMOVE] Shutting down {} worker: {}",
            if worker.worker_type == WorkerType::Decision {
                "Decision"
            } else {
                "Analysis"
            },
            worker.id
        );

        // Signal worker to shut down
        worker.shutdown_notify.notify_one();

        // Wait for worker to complete with timeout
        let timeout_duration = Duration::from_secs(30);
        match tokio::time::timeout(timeout_duration, worker.handle).await {
            Ok(Ok(())) => {
                info!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} shut down successfully",
                    worker.id
                );
            }
            Ok(Err(e)) => {
                warn!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} completed with error: {}",
                    worker.id, e
                );
            }
            Err(_) => {
                warn!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} shutdown timed out after {:?}",
                    worker.id, timeout_duration
                );
            }
        }
    }

    /// Get current worker statistics broken down by type
    pub async fn get_worker_stats(&self) -> WorkerStats {
        let workers = self.active_workers.lock().await;
        let ollama_decision_count = workers
            .iter()
            .filter(|w| w.model_type == ModelType::Ollama && w.worker_type == WorkerType::Decision)
            .count();
        let ollama_analysis_count = workers
            .iter()
            .filter(|w| w.model_type == ModelType::Ollama && w.worker_type == WorkerType::Analysis)
            .count();
        let openai_decision_count = workers
            .iter()
            .filter(|w| w.model_type == ModelType::OpenAI && w.worker_type == WorkerType::Decision)
            .count();
        let openai_analysis_count = workers
            .iter()
            .filter(|w| w.model_type == ModelType::OpenAI && w.worker_type == WorkerType::Analysis)
            .count();

        WorkerStats {
            total_workers: workers.len(),
            ollama_decision_workers: ollama_decision_count,
            ollama_analysis_workers: ollama_analysis_count,
            openai_decision_workers: openai_decision_count,
            openai_analysis_workers: openai_analysis_count,
        }
    }

    /// Shutdown all workers
    pub async fn shutdown(&self) {
        info!("[MODEL_SCALING] Shutting down pool manager");

        // Signal scaling loop to stop
        self.shutdown_notify.notify_one();

        // Shutdown all workers
        let mut workers = self.active_workers.lock().await;
        let worker_list = std::mem::take(&mut *workers);
        drop(workers);

        for worker in worker_list {
            self.remove_worker(worker).await;
        }

        info!("[MODEL_SCALING] Pool manager shutdown complete");
    }
}

/// Simplified struct for the scaling loop to avoid complex cloning
struct PoolManagerLoop {
    active_workers: Arc<Mutex<Vec<WorkerHandle>>>,
    openai_decision_configs: Vec<WorkerConfig>,
    openai_analysis_configs: Vec<WorkerConfig>,
    scaling_config: ScalingConfig,
    database: Arc<Database>,
    topics: Vec<String>,
    slack_token: String,
    slack_channel: String,
    last_decision_scaling: Arc<Mutex<Option<Instant>>>,
    last_analysis_scaling: Arc<Mutex<Option<Instant>>>,
    shutdown_notify: Arc<Notify>,
}

impl PoolManagerLoop {
    async fn evaluate_scaling(&self) -> Result<()> {
        // Get queue pressures for each worker type
        let queue_sizes = self.database.get_queue_sizes().await?;
        let decision_pressure = queue_sizes.rss; // Decision workers handle RSS queue
        let analysis_pressure = queue_sizes.life_safety * 3 + queue_sizes.matched_topics * 2; // Analysis workers handle weighted Life Safety + Matched Topics

        info!("[MODEL_SCALING] [QUEUE_PRESSURE] Current pressures - Decision: {} (RSS queue), Analysis: {} (Life Safety + Matched Topics)", 
              decision_pressure, analysis_pressure);

        // Scale Decision workers independently
        self.evaluate_decision_scaling(decision_pressure).await?;

        // Scale Analysis workers independently
        self.evaluate_analysis_scaling(analysis_pressure).await?;

        Ok(())
    }

    async fn evaluate_decision_scaling(&self, queue_pressure: u32) -> Result<()> {
        let last_op = self.last_decision_scaling.lock().await;
        if let Some(last_time) = *last_op {
            if last_time.elapsed() < self.scaling_config.scaling_cooldown {
                return Ok(()); // Still in cooldown
            }
        }

        let workers = self.active_workers.lock().await;
        let current_openai_count = workers
            .iter()
            .filter(|w| w.worker_type == WorkerType::Decision && w.model_type == ModelType::OpenAI)
            .count() as u32;

        if queue_pressure > self.scaling_config.scale_up_threshold
            && current_openai_count < self.scaling_config.max_openai_workers
        {
            if let Some(config) = self
                .openai_decision_configs
                .get(current_openai_count as usize)
            {
                drop(workers);
                drop(last_op);

                info!("[MODEL_SCALING] [WORKER_ADD] Scaling up Decision workers: Adding OpenAI worker (pressure: {}, threshold: {})",
                      queue_pressure, self.scaling_config.scale_up_threshold);

                if let Err(e) = self.add_worker_loop(config.clone()).await {
                    error!(
                        "[MODEL_SCALING] [WORKER_ADD] Failed to add OpenAI Decision worker: {}",
                        e
                    );
                } else {
                    let mut last_op = self.last_decision_scaling.lock().await;
                    *last_op = Some(Instant::now());
                }
            }
        } else if queue_pressure < self.scaling_config.scale_down_threshold
            && current_openai_count > 0
        {
            let mut workers = self.active_workers.lock().await;
            if let Some(worker_to_remove) = workers.iter().position(|w| {
                w.worker_type == WorkerType::Decision && w.model_type == ModelType::OpenAI
            }) {
                let worker = workers.remove(worker_to_remove);
                drop(workers);
                drop(last_op);

                info!("[MODEL_SCALING] [WORKER_REMOVE] Scaling down Decision workers: Removing OpenAI worker {} (pressure: {}, threshold: {})",
                      worker.id, queue_pressure, self.scaling_config.scale_down_threshold);

                self.remove_worker_loop(worker).await;
                let mut last_op = self.last_decision_scaling.lock().await;
                *last_op = Some(Instant::now());
            }
        }

        Ok(())
    }

    async fn evaluate_analysis_scaling(&self, queue_pressure: u32) -> Result<()> {
        let last_op = self.last_analysis_scaling.lock().await;
        if let Some(last_time) = *last_op {
            if last_time.elapsed() < self.scaling_config.scaling_cooldown {
                return Ok(()); // Still in cooldown
            }
        }

        let workers = self.active_workers.lock().await;
        let current_openai_count = workers
            .iter()
            .filter(|w| w.worker_type == WorkerType::Analysis && w.model_type == ModelType::OpenAI)
            .count() as u32;

        if queue_pressure > self.scaling_config.scale_up_threshold
            && current_openai_count < self.scaling_config.max_openai_workers
        {
            if let Some(config) = self
                .openai_analysis_configs
                .get(current_openai_count as usize)
            {
                drop(workers);
                drop(last_op);

                info!("[MODEL_SCALING] [WORKER_ADD] Scaling up Analysis workers: Adding OpenAI worker (pressure: {}, threshold: {})",
                      queue_pressure, self.scaling_config.scale_up_threshold);

                if let Err(e) = self.add_worker_loop(config.clone()).await {
                    error!(
                        "[MODEL_SCALING] [WORKER_ADD] Failed to add OpenAI Analysis worker: {}",
                        e
                    );
                } else {
                    let mut last_op = self.last_analysis_scaling.lock().await;
                    *last_op = Some(Instant::now());
                }
            }
        } else if queue_pressure < self.scaling_config.scale_down_threshold
            && current_openai_count > 0
        {
            let mut workers = self.active_workers.lock().await;
            if let Some(worker_to_remove) = workers.iter().position(|w| {
                w.worker_type == WorkerType::Analysis && w.model_type == ModelType::OpenAI
            }) {
                let worker = workers.remove(worker_to_remove);
                drop(workers);
                drop(last_op);

                info!("[MODEL_SCALING] [WORKER_REMOVE] Scaling down Analysis workers: Removing OpenAI worker {} (pressure: {}, threshold: {})",
                      worker.id, queue_pressure, self.scaling_config.scale_down_threshold);

                self.remove_worker_loop(worker).await;
                let mut last_op = self.last_analysis_scaling.lock().await;
                *last_op = Some(Instant::now());
            }
        }

        Ok(())
    }

    async fn add_worker_loop(&self, config: WorkerConfig) -> Result<()> {
        let worker_id = format!(
            "{:?}-{}-{}",
            config.worker_type,
            config.model,
            chrono::Utc::now().timestamp_millis()
        );

        let model_type = match config.llm_client {
            LLMClient::Ollama(_) => ModelType::Ollama,
            LLMClient::OpenAI(_) => ModelType::OpenAI,
        };

        let shutdown_notify = Arc::new(Notify::new());
        let worker_shutdown = Arc::clone(&shutdown_notify);

        let topics = self.topics.clone();
        let slack_token = self.slack_token.clone();
        let slack_channel = self.slack_channel.clone();

        let handle = match config.worker_type {
            WorkerType::Decision => {
                let config_clone = config.clone();
                let worker_id_clone = worker_id.clone();
                tokio::spawn(async move {
                    let thread_name = format!("Dynamic Decision Worker {}", worker_id_clone);
                    info!("[MODEL_SCALING] [WORKER_ADD] Starting {}", thread_name);

                    tokio::select! {
                        result = decision_worker::decision_loop(
                            0, // Dynamic workers use ID 0
                            &topics,
                            &config_clone.llm_client,
                            &config_clone.model,
                            config_clone.temperature,
                            &slack_token,
                            &slack_channel,
                            config_clone.no_think,
                            config_clone.model_config,
                            config_clone.shared_rate_limiter,
                        ) => {
                            match result {
                                Ok(_) => info!("[MODEL_SCALING] [WORKER_REMOVE] {} completed successfully", thread_name),
                                Err(e) => error!("[MODEL_SCALING] [WORKER_REMOVE] {} failed: {}", thread_name, e),
                            }
                        }
                        _ = worker_shutdown.notified() => {
                            info!("[MODEL_SCALING] [WORKER_REMOVE] {} shutting down on request", thread_name);
                        }
                    }
                })
            }
            WorkerType::Analysis => {
                let config_clone = config.clone();
                let worker_id_clone = worker_id.clone();
                tokio::spawn(async move {
                    let thread_name = format!("Dynamic Analysis Worker {}", worker_id_clone);
                    info!("[MODEL_SCALING] [WORKER_ADD] Starting {}", thread_name);

                    tokio::select! {
                        result = analysis_worker::analysis_loop(
                            0, // Dynamic workers use ID 0
                            &topics,
                            &config_clone.llm_client,
                            &config_clone.model,
                            &slack_token,
                            &slack_channel,
                            config_clone.temperature,
                            config_clone.fallback,
                            config_clone.model_config,
                            config_clone.no_think,
                        ) => {
                            match result {
                                Ok(_) => info!("[MODEL_SCALING] [WORKER_REMOVE] {} completed successfully", thread_name),
                                Err(e) => error!("[MODEL_SCALING] [WORKER_REMOVE] {} failed: {}", thread_name, e),
                            }
                        }
                        _ = worker_shutdown.notified() => {
                            info!("[MODEL_SCALING] [WORKER_REMOVE] {} shutting down on request", thread_name);
                        }
                    }
                })
            }
        };

        let worker = WorkerHandle {
            id: worker_id.clone(),
            worker_type: config.worker_type,
            model_type,
            handle,
            shutdown_notify,
        };

        let mut workers = self.active_workers.lock().await;
        workers.push(worker);

        info!(
            "[MODEL_SCALING] [WORKER_ADD] Added {} {} worker: {}",
            if model_type == ModelType::OpenAI {
                "OpenAI"
            } else {
                "Ollama"
            },
            if config.worker_type == WorkerType::Decision {
                "Decision"
            } else {
                "Analysis"
            },
            worker_id
        );

        Ok(())
    }

    async fn remove_worker_loop(&self, worker: WorkerHandle) {
        info!(
            "[MODEL_SCALING] [WORKER_REMOVE] Shutting down {} worker: {}",
            if worker.worker_type == WorkerType::Decision {
                "Decision"
            } else {
                "Analysis"
            },
            worker.id
        );

        // Signal worker to shut down
        worker.shutdown_notify.notify_one();

        // Wait for worker to complete with timeout
        let timeout_duration = Duration::from_secs(30);
        match tokio::time::timeout(timeout_duration, worker.handle).await {
            Ok(Ok(())) => {
                info!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} shut down successfully",
                    worker.id
                );
            }
            Ok(Err(e)) => {
                warn!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} completed with error: {}",
                    worker.id, e
                );
            }
            Err(_) => {
                warn!(
                    "[MODEL_SCALING] [WORKER_REMOVE] Worker {} shutdown timed out after {:?}",
                    worker.id, timeout_duration
                );
            }
        }
    }
}

/// Statistics about current worker pool (broken down by type)
#[derive(Debug, Clone)]
pub struct WorkerStats {
    pub total_workers: usize,
    pub ollama_decision_workers: usize,
    pub ollama_analysis_workers: usize,
    pub openai_decision_workers: usize,
    pub openai_analysis_workers: usize,
}

/// Helper functions for creating worker configurations
pub fn create_decision_ollama_config(
    host: String,
    port: u16,
    model: String,
    no_think: bool,
    temperature: f32,
    model_config: Option<ModelConfig>,
) -> WorkerConfig {
    WorkerConfig {
        worker_type: WorkerType::Decision,
        llm_client: LLMClient::Ollama(Ollama::new(host, port)),
        model,
        no_think,
        fallback: None,
        temperature,
        model_config,
        shared_rate_limiter: None,
    }
}

pub fn create_decision_openai_config(
    api_key: String,
    model: String,
    temperature: f32,
    model_config: Option<ModelConfig>,
) -> WorkerConfig {
    let config = OpenAIConfig::new().with_api_key(&api_key);
    let client = OpenAIClient::with_config(config);

    WorkerConfig {
        worker_type: WorkerType::Decision,
        llm_client: LLMClient::OpenAI(client),
        model,
        no_think: false, // OpenAI doesn't support no_think
        fallback: None,
        temperature,
        model_config,
        shared_rate_limiter: None,
    }
}

pub fn create_analysis_ollama_config(
    host: String,
    port: u16,
    model: String,
    no_think: bool,
    fallback: Option<FallbackConfig>,
    temperature: f32,
    model_config: Option<ModelConfig>,
) -> WorkerConfig {
    WorkerConfig {
        worker_type: WorkerType::Analysis,
        llm_client: LLMClient::Ollama(Ollama::new(host, port)),
        model,
        no_think,
        fallback,
        temperature,
        model_config,
        shared_rate_limiter: None,
    }
}

pub fn create_analysis_openai_config(
    api_key: String,
    model: String,
    fallback: Option<FallbackConfig>,
    temperature: f32,
    model_config: Option<ModelConfig>,
) -> WorkerConfig {
    let config = OpenAIConfig::new().with_api_key(&api_key);
    let client = OpenAIClient::with_config(config);

    WorkerConfig {
        worker_type: WorkerType::Analysis,
        llm_client: LLMClient::OpenAI(client),
        model,
        no_think: false, // OpenAI doesn't support no_think
        fallback,
        temperature,
        model_config,
        shared_rate_limiter: None,
    }
}
