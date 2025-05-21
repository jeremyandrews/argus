use anyhow::Result;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info};

use crate::db::core::Database;
use crate::llm::generate_text_response;
use crate::util::{parse_places_data_detailed, parse_places_data_hierarchical};
use crate::workers::common::{build_connection_info, FeedItem, ProcessItemParams};
use crate::{
    FallbackConfig, LLMClient, LLMParamsBase, TextLLMParams, ThinkingModelConfig, WorkerDetail,
    TARGET_LLM_REQUEST,
};

/// Enum to represent the current mode of the Analysis Worker
enum Mode {
    Analysis,
    FallbackDecision,
}

/// Main analysis loop function with fallback mechanism
pub async fn analysis_loop(
    worker_id: i16,
    topics: &[String],
    llm_client: &LLMClient,
    model: &str,
    slack_token: &str,
    default_slack_channel: &str,
    temperature: f32,
    fallback: Option<FallbackConfig>,
    thinking_config: Option<ThinkingModelConfig>,
    no_think: bool,
) -> Result<()> {
    let db = Database::instance().await;
    let mut llm_params = TextLLMParams {
        base: LLMParamsBase {
            llm_client: llm_client.clone(),
            model: model.to_string(),
            temperature,
            thinking_config: thinking_config.clone(),
            no_think,
        },
    };

    let mut mode = Mode::Analysis;
    let mut fallback_start_time: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();

    // Extract connection info from the LLM client
    let connection_info = build_connection_info(llm_client, worker_id, "ANALYSIS_OLLAMA_CONFIGS");

    let mut worker_detail = WorkerDetail {
        name: "analysis worker".to_string(),
        id: worker_id,
        model: model.to_string(),
        connection_info,
    };

    // If analysis_worker will switch to a decision_worker, we need this.
    let places = match parse_places_data_hierarchical() {
        Ok(hierarchy) => hierarchy,
        Err(err) => panic!("Error: {}", err),
    };
    info!("loaded places: {:#?}", places);

    // And we need this to analyze life safety threats.
    let places_detailed = match parse_places_data_detailed() {
        Ok(hierarchy) => hierarchy,
        Err(err) => panic!("Error: {}", err),
    };
    info!("loaded places_detailed: {:#?}", places_detailed);

    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: starting analysis_loop using {:?}.", worker_detail.name, worker_detail.id, worker_detail.model, llm_client);

    loop {
        match mode {
            Mode::Analysis => {
                // Attempt to process an analysis item
                let processed = super::processing::process_analysis_item(
                    &worker_detail,
                    &mut llm_params,
                    &db,
                    slack_token,
                    default_slack_channel,
                    &places_detailed,
                )
                .await;

                if processed {
                    last_activity = Instant::now();
                }

                // Check if idle for over 10 minutes
                if last_activity.elapsed() > Duration::from_secs(600) {
                    if let Some(fallback_config) = fallback.clone() {
                        info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: idle for more than 10 minutes ({:#?}), switching to Decision Worker mode with model {}.", worker_detail.name, worker_detail.id, worker_detail.model, last_activity.elapsed(), fallback_config.model);
                        mode = Mode::FallbackDecision;
                        fallback_start_time = Some(Instant::now());

                        // Update active model to fallback model
                        worker_detail.model = fallback_config.model.to_string();

                        // Update LLM params to use fallback model (no thinking config in fallback)
                        llm_params = TextLLMParams {
                            base: LLMParamsBase {
                                llm_client: fallback_config.llm_client.clone(),
                                model: fallback_config.model.clone(),
                                temperature,
                                thinking_config: None, // No thinking in fallback mode
                                no_think: fallback_config.no_think,
                            },
                        };

                        // Wait for the new model to be operational
                        if let Err(_) = wait_for_model_ready(
                            &fallback_config.model,
                            &mut llm_params,
                            &worker_detail,
                        )
                        .await
                        {
                            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Failed to switch to fallback model '{}'. Continuing in Analysis mode.", worker_detail.name, worker_detail.id, worker_detail.model, fallback_config.model);
                            mode = Mode::Analysis;
                            fallback_start_time = None;
                            worker_detail.model = model.to_string();
                            llm_params = TextLLMParams {
                                base: LLMParamsBase {
                                    llm_client: llm_client.clone(),
                                    model: model.to_string(),
                                    temperature,
                                    thinking_config: thinking_config.clone(),
                                    no_think,
                                },
                            };
                            // Give time for the original model to restore.
                            let _ =
                                wait_for_model_ready(&model, &mut llm_params, &worker_detail).await;
                        }
                    }
                }

                // Sleep briefly to prevent tight loop
                sleep(Duration::from_secs(2)).await;
            }
            Mode::FallbackDecision => {
                // Existing fallback processing logic...
                if let Some(_fallback_config) = fallback.clone() {
                    // Check if fallback duration has elapsed
                    if let Some(start_time) = fallback_start_time {
                        if start_time.elapsed() > Duration::from_secs(300) {
                            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: switching back from Decision Worker after 5 minutes, returning to mode with model {}.", worker_detail.name, worker_detail.id, worker_detail.model, model);
                            mode = Mode::Analysis;
                            fallback_start_time = None;

                            // Restore original LLM params with thinking config
                            llm_params = TextLLMParams {
                                base: LLMParamsBase {
                                    llm_client: llm_client.clone(),
                                    model: model.to_string(),
                                    temperature,
                                    thinking_config: thinking_config.clone(),
                                    no_think,
                                },
                            };

                            worker_detail.model = model.to_string();

                            // Wait for the original model to be operational
                            let _ =
                                wait_for_model_ready(model, &mut llm_params, &worker_detail).await;
                            continue;
                        }
                    }

                    let places_clone = places.clone();

                    // Process a single Decision task
                    match db.fetch_and_delete_url_from_rss_queue("random").await {
                        Ok(Some((url, title, pub_date))) => {
                            if url.trim().is_empty() {
                                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping empty URL in RSS queue.", worker_detail.name, worker_detail.id, worker_detail.model);
                            } else {
                                let item = FeedItem {
                                    url,
                                    title,
                                    pub_date,
                                };

                                let mut params = ProcessItemParams {
                                    topics,
                                    llm_client: &llm_params.base.llm_client,
                                    model: &llm_params.base.model,
                                    temperature: llm_params.base.temperature,
                                    db: &db,
                                    slack_token,
                                    slack_channel: default_slack_channel,
                                    places: places_clone,
                                };

                                // Process the item using the decision worker's process_item function
                                crate::workers::decision::processing::process_item(
                                    item,
                                    &mut params,
                                    &worker_detail,
                                )
                                .await;
                            }
                        }
                        Ok(None) => {
                            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no URLs in rss_queue, sleeping 1 minute.", worker_detail.name, worker_detail.id, worker_detail.model);
                            sleep(Duration::from_secs(60)).await;
                        }
                        Err(e) => {
                            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error fetching URL from rss_queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
                            sleep(Duration::from_secs(5)).await; // Wait and retry
                        }
                    }

                    // Sleep briefly to prevent tight loop
                    sleep(Duration::from_secs(2)).await;
                } else {
                    // No fallback configured; remain in Analysis mode
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no Decision fallback configured, remaining in analysis mode.", worker_detail.name, worker_detail.id, worker_detail.model);
                    mode = Mode::Analysis;
                }
            }
        }

        // After handling the current mode, check if fallback time has expired to switch back
        if let Mode::FallbackDecision = mode {
            if let Some(start_time) = fallback_start_time {
                if start_time.elapsed() > Duration::from_secs(900) {
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: switching back from Decision Worker after 15 minutes, returning to mode with model {}.", worker_detail.name, worker_detail.id, worker_detail.model, model);
                    mode = Mode::Analysis;
                    fallback_start_time = None;

                    // Update active model to original model
                    worker_detail.model = model.to_string();

                    // Restore original LLM params with thinking config
                    llm_params = TextLLMParams {
                        base: LLMParamsBase {
                            llm_client: llm_client.clone(),
                            model: model.to_string(),
                            temperature,
                            thinking_config: thinking_config.clone(),
                            no_think,
                        },
                    };

                    // Wait for the original model to be operational
                    let _ = wait_for_model_ready(model, &mut llm_params, &worker_detail).await;
                }
            }
        }
    }
}

/// Waits until the specified model is operational by repeatedly sending a test prompt.
///
/// # Arguments
///
/// * `model` - The name of the model to test.
/// * `llm_params` - Mutable reference to the current LLM parameters.
/// * `worker_detail` - Reference to the worker's details for logging purposes.
async fn wait_for_model_ready(
    model: &str,
    llm_params: &mut TextLLMParams,
    worker_detail: &WorkerDetail,
) -> Result<(), ()> {
    let test_prompt = "Are you operational? Answer yes or no.";
    let retry_delay = Duration::from_secs(5);
    let max_retries = 60;
    let mut attempts = 0;

    loop {
        attempts += 1;
        match generate_text_response(test_prompt, llm_params, worker_detail).await {
            Some(response) => {
                info!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Model '{}' responded to readiness check: {}.",
                    worker_detail.name,
                    worker_detail.id,
                    worker_detail.model,
                    model,
                    response,
                );
                return Ok(());
            }
            None => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Model '{}' not ready yet (Attempt {}/{}).",
                    worker_detail.name,
                    worker_detail.id,
                    worker_detail.model,
                    model,
                    attempts,
                    max_retries,
                );
                if attempts >= max_retries {
                    error!(
                        target: TARGET_LLM_REQUEST,
                        "[{} {} {}]: Model '{}' failed to become operational after {} attempts.",
                        worker_detail.name,
                        worker_detail.id,
                        worker_detail.model,
                        model,
                        max_retries,
                    );
                    return Err(());
                }
                sleep(retry_delay).await;
            }
        }
    }
}
