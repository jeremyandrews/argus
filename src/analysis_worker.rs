use anyhow::Result;
use serde_json::json;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::app::util::send_to_app;
use crate::db::Database;
use crate::decision_worker::FeedItem;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::slack::send_to_slack;
use crate::{FallbackConfig, LLMClient, LLMParams, WorkerDetail, TARGET_LLM_REQUEST};

// Import necessary items from decision_worker
use crate::decision_worker::ProcessItemParams;

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
) -> Result<()> {
    let db = Database::instance().await;
    let mut llm_params = LLMParams {
        llm_client: llm_client.clone(),
        model: model.to_string(),
        temperature,
    };

    let mut mode = Mode::Analysis;
    let mut fallback_start_time: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();

    let mut worker_detail = WorkerDetail {
        name: "analysis worker".to_string(),
        id: worker_id,
        model: model.to_string(),
    };

    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: starting analysis_loop using {:?}.", worker_detail.name, worker_detail.id, worker_detail.model, llm_client);

    loop {
        match mode {
            Mode::Analysis => {
                // Attempt to process an analysis item
                let processed = process_analysis_item(
                    &worker_detail,
                    &mut llm_params,
                    &db,
                    slack_token,
                    default_slack_channel,
                    temperature,
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

                        // Update LLM params to use fallback model
                        llm_params = LLMParams {
                            llm_client: fallback_config.llm_client.clone(),
                            model: fallback_config.model.clone(),
                            temperature,
                        };

                        // Wait for the new model to be operational
                        // @TODO: Skip this for OpenAI?
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
                            llm_params = LLMParams {
                                llm_client: llm_client.clone(),
                                model: model.to_string(),
                                temperature,
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

                            // Restore original LLM params
                            llm_params = LLMParams {
                                llm_client: llm_client.clone(),
                                model: model.to_string(),
                                temperature,
                            };

                            worker_detail.model = model.to_string();

                            // Wait for the original model to be operational
                            let _ =
                                wait_for_model_ready(model, &mut llm_params, &worker_detail).await;
                            continue;
                        }
                    }

                    // Process a single Decision task
                    process_decision_item(
                        &worker_detail,
                        &mut llm_params,
                        &db,
                        slack_token,
                        default_slack_channel,
                        topics,
                    )
                    .await;

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

                    // Restore original LLM params
                    llm_params = LLMParams {
                        llm_client: llm_client.clone(),
                        model: model.to_string(),
                        temperature,
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
    llm_params: &mut LLMParams,
    worker_detail: &WorkerDetail,
) -> Result<(), ()> {
    let test_prompt = "Are you operational? Answer yes or no.";
    let retry_delay = Duration::from_secs(5);
    let max_retries = 60;
    let mut attempts = 0;

    loop {
        attempts += 1;
        match generate_llm_response(test_prompt, llm_params, worker_detail).await {
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

/// Function to process a single analysis item.
/// Returns true if an item was processed, false otherwise.
async fn process_analysis_item(
    worker_detail: &WorkerDetail,
    llm_params: &mut LLMParams,
    db: &Database,
    slack_token: &str,
    slack_channel: &str,
    _temperature: f32,
) -> bool {
    // Fetch from life_safety_queue
    match db.fetch_and_delete_from_life_safety_queue().await {
        Ok(Some((
            article_url,
            article_title,
            article_text,
            article_html,
            article_hash,
            title_domain_hash,
            affected_regions,
            affected_people,
            affected_places_set,
            non_affected_people,
            non_affected_places,
        ))) => {
            let start_time = Instant::now();

            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from life safety queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

            // Check if the article has already been processed
            if db.has_hash(&article_hash).await.unwrap_or(false)
                || db
                    .has_title_domain_hash(&title_domain_hash)
                    .await
                    .unwrap_or(false)
            {
                info!(
                    target: TARGET_LLM_REQUEST,
                    "Article with hash {} or title_domain_hash {} was already processed. Skipping.",
                    article_hash, title_domain_hash
                );
                // No item processed, this was a duplicate.
                return false;
            }

            let mut llm_params_clone = llm_params.clone();

            let (
                summary,
                tiny_summary,
                tiny_title,
                critical_analysis,
                logical_fallacies,
                source_analysis,
                _relation_to_topic,
            ) = process_analysis(
                &article_text,
                &article_html,
                &article_url,
                None,
                &mut llm_params_clone,
                worker_detail,
            )
            .await;

            let mut relation_to_topic_str = String::new();

            // Generate relation to topic (affected and non-affected summary)
            let affected_summary;

            // For affected places
            if !affected_people.is_empty() {
                affected_summary = format!(
                    "This article affects these people in {}: {}",
                    affected_regions
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    affected_people
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let affected_places_str = affected_places_set
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                let how_prompt =
                    prompts::how_does_it_affect_prompt(&article_text, &affected_places_str);
                let how_response = generate_llm_response(&how_prompt, llm_params, worker_detail)
                    .await
                    .unwrap_or_default();
                relation_to_topic_str
                    .push_str(&format!("\n\n{}\n\n{}", affected_summary, how_response));
            } else {
                affected_summary = String::new();
            }

            // For non-affected places
            let non_affected_summary;
            if !non_affected_people.is_empty() {
                non_affected_summary = format!(
                    "This article does not affect these people in {}: {}",
                    affected_regions
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    non_affected_people
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let why_not_prompt = prompts::why_not_affect_prompt(
                    &article_text,
                    &non_affected_places
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                let why_not_response =
                    generate_llm_response(&why_not_prompt, llm_params, worker_detail)
                        .await
                        .unwrap_or_default();
                relation_to_topic_str.push_str(&format!(
                    "\n\n{}\n\n{}",
                    non_affected_summary, why_not_response
                ));
            }

            if !summary.is_empty()
                && !tiny_summary.is_empty()
                && !critical_analysis.is_empty()
                && !logical_fallacies.is_empty()
                && !relation_to_topic_str.is_empty()
            {
                let detailed_response_json = json!({
                    "topic": "Alert",
                    "title": article_title,
                    "url": article_url,
                    "affected": affected_summary,
                    "article_body": article_text,
                    "tiny_summary": tiny_summary,
                    "tiny_title": tiny_title,
                    "summary": summary,
                    "critical_analysis": critical_analysis,
                    "logical_fallacies": logical_fallacies,
                    "relation_to_topic": relation_to_topic_str,
                    "source_analysis": source_analysis,
                    "elapsed_time": start_time.elapsed().as_secs_f64(),
                    "model": llm_params.model
                });

                // Save the article first
                if let Err(e) = db
                    .add_article(
                        &article_url,
                        true,
                        None,
                        Some(&detailed_response_json.to_string()),
                        Some(&tiny_summary),
                        Some(&article_hash),
                        Some(&title_domain_hash),
                        None, // R2 URL will be updated later
                    )
                    .await
                {
                    error!(
                        target: TARGET_LLM_REQUEST,
                        "Failed to save article to database: {:?}", e
                    );
                    return false; // Skip processing if saving fails
                }

                // Send notification to app
                if let Some(r2_url) = send_to_app(&detailed_response_json, "high").await {
                    // Update the article with R2 details
                    if let Err(e) = db
                        .update_article_with_r2_details(&article_url, &r2_url)
                        .await
                    {
                        error!(
                            target: TARGET_LLM_REQUEST,
                            "Failed to update R2 details in database: {:?}", e
                        );
                    }
                } else {
                    warn!("failed to send Alert: {} to app...", article_url);
                }

                // Send notification to Slack
                send_to_slack(
                    &format!("*<{}|{}>*", article_url, article_title),
                    &detailed_response_json.to_string(),
                    slack_token,
                    slack_channel,
                )
                .await;
            }
            // An item was processed, return to process another.
            return true;
        }
        Ok(None) => {
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no items in life safety queue.", worker_detail.name, worker_detail.id, worker_detail.model);

            // Process matched topics queue as before
            match db.fetch_and_delete_from_matched_topics_queue().await {
                Ok(Some((
                    article_text,
                    article_html,
                    article_url,
                    article_title,
                    article_hash,
                    title_domain_hash,
                    topic,
                ))) => {
                    let mut llm_params_clone = llm_params.clone();

                    let start_time = std::time::Instant::now();

                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from matched topics queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

                    let (
                        summary,
                        tiny_summary,
                        tiny_title,
                        critical_analysis,
                        logical_fallacies,
                        source_analysis,
                        relation,
                    ) = process_analysis(
                        &article_text,
                        &article_html,
                        &article_url,
                        Some(&topic),
                        &mut llm_params_clone,
                        worker_detail,
                    )
                    .await;

                    if !summary.is_empty()
                        && !tiny_summary.is_empty()
                        && !critical_analysis.is_empty()
                        && !logical_fallacies.is_empty()
                    {
                        let response_json = json!({
                            "topic": topic,
                            "title": article_title,
                            "url": article_url,
                            "article_body": article_text,
                            "tiny_summary": tiny_summary,
                            "tiny_title": tiny_title,
                            "summary": summary,
                            "critical_analysis": critical_analysis,
                            "logical_fallacies": logical_fallacies,
                            "relation_to_topic": relation,
                            "source_analysis": source_analysis,
                            "elapsed_time": start_time.elapsed().as_secs_f64(),
                            "model": llm_params.model
                        });

                        // Save the article first
                        if let Err(e) = db
                            .add_article(
                                &article_url,
                                true,
                                Some(&topic),
                                Some(&response_json.to_string()),
                                Some(&tiny_summary),
                                Some(&article_hash),
                                Some(&title_domain_hash),
                                None, // Placeholder for R2 URL, will update later
                            )
                            .await
                        {
                            error!(
                                target: TARGET_LLM_REQUEST,
                                "Failed to save article to database: {:?}", e
                            );
                            return false; // Skip processing if saving fails
                        }

                        // Send notification to app
                        if let Some(r2_url) = send_to_app(&response_json, "low").await {
                            // Update the article with R2 details
                            if let Err(e) = db
                                .update_article_with_r2_details(&article_url, &r2_url)
                                .await
                            {
                                error!(
                                    target: TARGET_LLM_REQUEST,
                                    "Failed to update R2 details in database: {:?}", e
                                );
                            }
                        } else {
                            warn!("failed to send analysis: {} to app...", article_url);
                        }

                        // Send notification to slack
                        send_to_slack(
                            &format!("*<{}|{}>*", article_url, article_title),
                            &response_json.to_string(),
                            slack_token,
                            slack_channel,
                        )
                        .await;

                        debug!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: sent analysis to slack: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, article_url
                        );
                        return true; // An item was processed
                    } else {
                        return false;
                    }
                }
                Ok(None) => {
                    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Matched Topics queue empty, sleeping 10 seconds...", worker_detail.name, worker_detail.id, worker_detail.model);
                    sleep(Duration::from_secs(10)).await;
                }
                Err(e) => {
                    error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error pulling from Matched topics queue: {:?}, sleeping 10 seconds...", worker_detail.name, worker_detail.id, worker_detail.model, e);
                    sleep(Duration::from_secs(5)).await;
                }
            }
            // If we get here, no item was processed, return to try again.
            return false;
        }
        Err(e) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error pulling from Life Safety queue: {:?}, sleeping 5 seconds...", worker_detail.name, worker_detail.id, worker_detail.model, e);
            // Sleep after a database error.
            sleep(Duration::from_secs(5)).await;
            // If we get here, no item was processed, return to try again.
            return false;
        }
    }
}

/// Function to process a single Decision task during fallback.
/// Returns true if an item was processed, false otherwise.
async fn process_decision_item(
    worker_detail: &WorkerDetail,
    llm_params: &mut LLMParams,
    db: &Database,
    slack_token: &str,
    slack_channel: &str,
    topics: &[String],
) {
    // Fetch from rss_queue similar to decision_worker::decision_loop
    match db.fetch_and_delete_url_from_rss_queue("random").await {
        Ok(Some((url, title))) => {
            if url.trim().is_empty() {
                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping empty URL in RSS queue.", worker_detail.name, worker_detail.id, worker_detail.model);
                return;
            }

            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: new URL: {} ({:?}).", worker_detail.name, worker_detail.id, worker_detail.model, url, title);

            let item = FeedItem {
                url: url.clone(),
                title,
            };

            let mut params = ProcessItemParams {
                topics,
                llm_client: &llm_params.llm_client,
                model: &llm_params.model,
                temperature: llm_params.temperature,
                db,
                slack_token,
                slack_channel,
                places: None, // No places needed for fallback Decision mode
            };

            // Reuse the existing process_item logic from decision_worker
            crate::decision_worker::process_item(item, &mut params, &worker_detail).await;
        }
        Ok(None) => {
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no URLs in rss_queue, sleeping 1 minute URL.", worker_detail.name, worker_detail.id, worker_detail.model);
            sleep(Duration::from_secs(60)).await;
        }
        Err(e) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error fetching URL from rss_queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            sleep(Duration::from_secs(5)).await; // Wait and retry
        }
    }
}

/// Function to perform the analysis on an article.
/// Returns a tuple containing various analysis results.
async fn process_analysis(
    article_text: &str,
    article_html: &str,
    article_url: &str,
    topic: Option<&str>,
    llm_params: &mut LLMParams,
    worker_detail: &WorkerDetail,
) -> (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
) {
    // Re-summarize the article with the analysis worker.
    let summary_prompt = prompts::summary_prompt(article_text);
    let summary = generate_llm_response(&summary_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_default();

    // Now perform the rest of the analysis.
    let tiny_summary_prompt = prompts::tiny_summary_prompt(&summary);
    let tiny_title_prompt = prompts::tiny_title_prompt(&summary);
    let critical_analysis_prompt = prompts::critical_analysis_prompt(article_text);
    let logical_fallacies_prompt = prompts::logical_fallacies_prompt(article_text);
    let source_analysis_prompt = prompts::source_analysis_prompt(article_html, article_url);

    let tiny_summary = generate_llm_response(&tiny_summary_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_default();
    let tiny_title = generate_llm_response(&tiny_title_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_default();
    let critical_analysis =
        generate_llm_response(&critical_analysis_prompt, llm_params, worker_detail)
            .await
            .unwrap_or_default();
    let logical_fallacies =
        generate_llm_response(&logical_fallacies_prompt, llm_params, worker_detail)
            .await
            .unwrap_or_default();
    let source_analysis = generate_llm_response(&source_analysis_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_default();

    let relation_response = if let Some(topic) = topic {
        let relation_prompt = prompts::relation_to_topic_prompt(article_text, topic);
        Some(
            generate_llm_response(&relation_prompt, &llm_params, worker_detail)
                .await
                .unwrap_or_default(),
        )
    } else {
        None
    };

    (
        summary,
        tiny_summary,
        tiny_title,
        critical_analysis,
        logical_fallacies,
        source_analysis,
        relation_response,
    )
}
