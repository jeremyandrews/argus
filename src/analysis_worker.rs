use serde_json::json;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::decision_worker::FeedItem;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::slack::send_to_slack;
use crate::{FallbackConfig, LLMClient, LLMParams, TARGET_LLM_REQUEST};

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
) {
    let db = Database::instance().await;
    let mut llm_params = LLMParams {
        llm_client: llm_client.clone(),
        model: model.to_string(),
        temperature,
    };

    let mut mode = Mode::Analysis;
    let mut fallback_start_time: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();

    info!(
        target: TARGET_LLM_REQUEST,
        "Analysis worker {}: starting analysis_loop in Analysis mode with model '{}'.",
        worker_id, model
    );
    debug!(
        "Analysis worker {} is running with model '{}' using {:?}.",
        worker_id, model, llm_client
    );

    loop {
        match mode {
            Mode::Analysis => {
                // Attempt to process an analysis item
                let processed = process_analysis_item(
                    worker_id,
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

                // Check if idle for over 2 minutes
                if last_activity.elapsed() > Duration::from_secs(120) {
                    if let Some(fallback_config) = fallback.clone() {
                        info!(
                            target: TARGET_LLM_REQUEST,
                            "Analysis worker {}: Idle for over 2 minutes. Switching to Decision mode with fallback model '{}'.",
                            worker_id, fallback_config.model
                        );
                        mode = Mode::FallbackDecision;
                        fallback_start_time = Some(Instant::now());

                        // Update LLM params to use fallback model
                        llm_params = LLMParams {
                            llm_client: fallback_config.llm_client.clone(),
                            model: fallback_config.model,
                            temperature,
                        };
                    }
                }

                // Sleep briefly to prevent tight loop
                sleep(Duration::from_secs(2)).await;
            }
            Mode::FallbackDecision => {
                if let Some(fallback_config) = fallback.clone() {
                    // Check if fallback duration has elapsed
                    if let Some(start_time) = fallback_start_time {
                        if start_time.elapsed() > Duration::from_secs(900) {
                            info!(
                                target: TARGET_LLM_REQUEST,
                                "Analysis worker {}: Fallback Decision mode for 15 minutes completed. Switching back to Analysis mode with model '{}'.",
                                worker_id, model
                            );
                            mode = Mode::Analysis;
                            fallback_start_time = None;

                            // Restore original LLM params
                            llm_params = LLMParams {
                                llm_client: llm_client.clone(),
                                model: model.to_string(),
                                temperature,
                            };
                            continue;
                        }
                    }

                    // Run Decision loop logic with fallback model
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Analysis worker {}: Running in Decision mode with model '{}'.",
                        worker_id, fallback_config.model
                    );

                    // Process a single Decision task
                    process_decision_item(
                        worker_id,
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
                    warn!(
                        target: TARGET_LLM_REQUEST,
                        "Analysis worker {}: No fallback configuration provided. Remaining in Analysis mode.",
                        worker_id
                    );
                    mode = Mode::Analysis;
                }
            }
        }

        // After handling the current mode, check if fallback time has expired to switch back
        if let Mode::FallbackDecision = mode {
            if let Some(start_time) = fallback_start_time {
                if start_time.elapsed() > Duration::from_secs(900) {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Analysis worker {}: Fallback Decision mode for 15 minutes completed. Switching back to Analysis mode with model '{}'.",
                        worker_id, model
                    );
                    mode = Mode::Analysis;
                    fallback_start_time = None;

                    // Restore original LLM params
                    llm_params = LLMParams {
                        llm_client: llm_client.clone(),
                        model: model.to_string(),
                        temperature,
                    };
                }
            }
        }
    }
}

/// Function to process a single analysis item.
/// Returns true if an item was processed, false otherwise.
async fn process_analysis_item(
    worker_id: i16,
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

            debug!(
                target: TARGET_LLM_REQUEST,
                "worker {}: Pulled item from life safety queue: {}",
                worker_id, article_url
            );

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
                return true;
            }

            let mut llm_params_clone = llm_params.clone();

            let (
                summary,
                tiny_summary,
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
            )
            .await;

            let mut relation_to_topic_str = String::new();

            // Generate relation to topic (affected and non-affected summary)
            let mut affected_summary = String::default();

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
                let how_response = generate_llm_response(&how_prompt, llm_params)
                    .await
                    .unwrap_or_default();
                relation_to_topic_str
                    .push_str(&format!("\n\n{}\n\n{}", affected_summary, how_response));
            }

            // For non-affected places
            let mut non_affected_summary = String::default();
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
                let why_not_response = generate_llm_response(&why_not_prompt, llm_params)
                    .await
                    .unwrap_or_default();
                relation_to_topic_str.push_str(&format!(
                    "\n\n{}\n\n{}",
                    non_affected_summary, why_not_response
                ));
            }

            if !summary.is_empty()
                || !critical_analysis.is_empty()
                || !logical_fallacies.is_empty()
                || !relation_to_topic_str.is_empty()
                || !source_analysis.is_empty()
            {
                let detailed_response_json = json!({
                    "topic": format!("{} {}", affected_summary, non_affected_summary),
                    "summary": summary,
                    "tiny_summary": tiny_summary,
                    "critical_analysis": critical_analysis,
                    "logical_fallacies": logical_fallacies,
                    "relation_to_topic": relation_to_topic_str,
                    "source_analysis": source_analysis,
                    "elapsed_time": start_time.elapsed().as_secs_f64(),
                    "model": llm_params.model
                });

                // Check again if the article hash already exists in the database before posting to Slack
                if db.has_hash(&article_hash).await.unwrap_or(false)
                    || db
                        .has_title_domain_hash(&title_domain_hash)
                        .await
                        .unwrap_or(false)
                {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Article with hash {} or title_domain_hash {} was already processed (third check). Skipping Slack post.",
                        article_hash, title_domain_hash
                    );
                    return true;
                }

                send_to_slack(
                    &format!("*<{}|{}>*", article_url, article_title),
                    &detailed_response_json.to_string(),
                    slack_token,
                    slack_channel,
                )
                .await;

                if let Err(e) = db
                    .add_article(
                        &article_url,
                        true,
                        None,
                        Some(&detailed_response_json.to_string()),
                        Some(&tiny_summary),
                        Some(&article_hash),
                        Some(&title_domain_hash),
                    )
                    .await
                {
                    error!(
                        target: TARGET_LLM_REQUEST,
                        "Failed to update database: {:?}", e
                    );
                }
            }
            // Log success, and go to next queue item.
            true
        }
        Ok(None) => {
            debug!(
                target: TARGET_LLM_REQUEST,
                "worker {}: No items in life safety queue.",
                worker_id
            );

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

                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "worker {}: Analyzing article from matched topics queue: {}",
                        worker_id,
                        article_url
                    );

                    let (
                        summary,
                        tiny_summary,
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
                    )
                    .await;

                    let response_json = json!({
                        "topic": topic,
                        "summary": summary,
                        "tiny_summary": tiny_summary,
                        "critical_analysis": critical_analysis,
                        "logical_fallacies": logical_fallacies,
                        "relation_to_topic": relation,
                        "source_analysis": source_analysis,
                        "elapsed_time": start_time.elapsed().as_secs_f64(),
                        "model": llm_params.model
                    });

                    send_to_slack(
                        &format!("*<{}|{}>*", article_url, article_title),
                        &response_json.to_string(),
                        slack_token,
                        slack_channel,
                    )
                    .await;

                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "worker {}: Successfully analyzed article and sent to Slack: {}",
                        worker_id,
                        article_url
                    );

                    if let Err(e) = db
                        .add_article(
                            &article_url,
                            true,
                            Some(&topic),
                            Some(&response_json.to_string()),
                            Some(&tiny_summary),
                            Some(&article_hash),
                            Some(&title_domain_hash),
                        )
                        .await
                    {
                        error!(target: TARGET_LLM_REQUEST, "Failed to update database: {:?}", e);
                    }
                }
                Ok(None) => {
                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "worker {}: No items in matched topics queue. Sleeping 10 seconds...",
                        worker_id
                    );
                    // Log success, and go to next queue item.
                    return true;
                }
                Err(e) => {
                    error!(
                        target: TARGET_LLM_REQUEST,
                        "worker {}: Error fetching from matched topics queue: {:?}", worker_id, e
                    );
                    // Sleep after a database error.
                    sleep(Duration::from_secs(5)).await;
                }
            }
            // Try again
            false
        }
        Err(e) => {
            error!(
                target: TARGET_LLM_REQUEST,
                "worker {}: Error fetching from life safety queue: {:?}",
                worker_id, e
            );
            // Sleep after a database error.
            sleep(Duration::from_secs(5)).await; // Wait and retry
                                                 // Then try again
            false
        }
    }
}

/// Function to process a single Decision task during fallback.
/// Returns true if an item was processed, false otherwise.
async fn process_decision_item(
    worker_id: i16,
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
                error!(
                    target: TARGET_LLM_REQUEST,
                    "worker {}: Found an empty URL in the queue, skipping...",
                    worker_id
                );
            }

            info!(
                target: TARGET_LLM_REQUEST,
                "worker {}: Moving on to a new URL: {} ({:?})",
                worker_id, url, title
            );

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
            crate::decision_worker::process_item(item, &mut params).await;
        }
        Ok(None) => {
            debug!(
                target: TARGET_LLM_REQUEST,
                "worker {}: No URLs to process in rss_queue. Sleeping for 1 minute before retrying.",
                worker_id
            );
            sleep(Duration::from_secs(60)).await;
        }
        Err(e) => {
            error!(
                target: TARGET_LLM_REQUEST,
                "worker {}: Error fetching URL from rss_queue: {:?}",
                worker_id, e
            );
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
) -> (String, String, String, String, String, Option<String>) {
    // Re-summarize the article with the analysis worker.
    let summary_prompt = prompts::summary_prompt(article_text);
    let summary = generate_llm_response(&summary_prompt, llm_params)
        .await
        .unwrap_or_default();

    // Now perform the rest of the analysis.
    let tiny_summary_prompt = prompts::tiny_summary_prompt(&summary);
    let critical_analysis_prompt = prompts::critical_analysis_prompt(article_text);
    let logical_fallacies_prompt = prompts::logical_fallacies_prompt(article_text);
    let source_analysis_prompt = prompts::source_analysis_prompt(article_html, article_url);

    let tiny_summary = generate_llm_response(&tiny_summary_prompt, llm_params)
        .await
        .unwrap_or_default();
    let critical_analysis = generate_llm_response(&critical_analysis_prompt, llm_params)
        .await
        .unwrap_or_default();
    let logical_fallacies = generate_llm_response(&logical_fallacies_prompt, llm_params)
        .await
        .unwrap_or_default();
    let source_analysis = generate_llm_response(&source_analysis_prompt, llm_params)
        .await
        .unwrap_or_default();

    let relation_response = if let Some(topic) = topic {
        let relation_prompt = prompts::relation_to_topic_prompt(article_text, topic);
        Some(
            generate_llm_response(&relation_prompt, &llm_params)
                .await
                .unwrap_or_default(),
        )
    } else {
        None
    };

    (
        summary,
        tiny_summary,
        critical_analysis,
        logical_fallacies,
        source_analysis,
        relation_response,
    )
}
