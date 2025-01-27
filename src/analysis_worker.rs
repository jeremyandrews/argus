use anyhow::Result;
use serde_json::json;
use std::collections::BTreeMap;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::app::util::send_to_app;
use crate::db::Database;
use crate::decision_worker::FeedItem;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::slack::send_to_slack;
use crate::util::{parse_places_data_detailed, parse_places_data_hierarchical};
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
        require_json: None,
    };

    let mut mode = Mode::Analysis;
    let mut fallback_start_time: Option<Instant> = None;
    let mut last_activity: Instant = Instant::now();

    let mut worker_detail = WorkerDetail {
        name: "analysis worker".to_string(),
        id: worker_id,
        model: model.to_string(),
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
                let processed = process_analysis_item(
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

                        // Update LLM params to use fallback model
                        llm_params = LLMParams {
                            llm_client: fallback_config.llm_client.clone(),
                            model: fallback_config.model.clone(),
                            temperature,
                            require_json: None,
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
                                require_json: None,
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
                                require_json: None,
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
                    process_decision_item(
                        &worker_detail,
                        &mut llm_params,
                        &db,
                        slack_token,
                        default_slack_channel,
                        topics,
                        places_clone,
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
                        require_json: None,
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
    places_detailed: &BTreeMap<
        String,
        BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>,
    >,
) -> bool {
    match db.fetch_and_delete_from_life_safety_queue().await {
        Ok(Some((
            article_url,
            article_title,
            article_text,
            _article_html,
            article_hash,
            title_domain_hash,
            threat_regions,
        ))) => {
            let start_time = Instant::now();
            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from life safety queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

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
                return false;
            }

            // Parse the JSON threat_regions
            let threat_regions: serde_json::Value = serde_json::from_str(&threat_regions)
                .unwrap_or_else(|_| json!({"impacted_regions": []}));

            let mut directly_affected_people = Vec::new();
            let mut indirectly_affected_people = Vec::new();

            // Iterate through the threat regions
            if let Some(impacted_regions) = threat_regions["impacted_regions"].as_array() {
                for region in impacted_regions {
                    let continent = region["continent"].as_str().unwrap_or("");
                    let country = region["country"].as_str().unwrap_or("");
                    let region_name = region["region"].as_str().unwrap_or("");

                    if let Some(countries) = places_detailed.get(continent) {
                        if let Some(regions) = countries.get(country) {
                            if let Some(cities) = regions.get(region_name) {
                                // Validate if the region truly has a threat
                                let region_prompt = prompts::region_threat_prompt(
                                    &article_text,
                                    region_name,
                                    country,
                                    continent,
                                );

                                let mut json_llm_params = llm_params.clone();
                                json_llm_params.require_json = Some(true);
                                let region_response = generate_llm_response(
                                    &region_prompt,
                                    &json_llm_params,
                                    worker_detail,
                                )
                                .await
                                .unwrap_or_default();

                                // Parse the response JSON
                                if let Ok(json_response) =
                                    serde_json::from_str::<serde_json::Value>(&region_response)
                                {
                                    if json_response["impacted_regions"]
                                        .as_array()
                                        .map_or(false, |regions| !regions.is_empty())
                                    {
                                        for (city_name, people) in cities.iter() {
                                            let city_prompt = prompts::city_threat_prompt(
                                                &article_text,
                                                city_name,
                                                region_name,
                                                country,
                                                continent,
                                            );
                                            let city_response = generate_llm_response(
                                                &city_prompt,
                                                llm_params,
                                                worker_detail,
                                            )
                                            .await
                                            .unwrap_or_default();
                                            if city_response.to_lowercase().contains("yes") {
                                                directly_affected_people.extend(people.clone());
                                            } else {
                                                indirectly_affected_people.extend(people.clone());
                                            }
                                        }
                                    }
                                } else {
                                    error!(
                                        target: TARGET_LLM_REQUEST,
                                        "Failed to parse JSON response for region. Response: {}",
                                        region_response
                                    );
                                }
                            }
                        }
                    }
                }
            }

            let affected_summary = if !directly_affected_people.is_empty() {
                format!(
                    "This article directly affects people in these locations: {}.",
                    directly_affected_people.join(", ")
                )
            } else {
                String::new()
            };

            let non_affected_summary = if !indirectly_affected_people.is_empty() {
                format!(
                    "This article indirectly affects people in these locations: {}.",
                    indirectly_affected_people.join(", ")
                )
            } else {
                String::new()
            };

            let detailed_response_json = json!({
                "article_url": article_url,
                "title": article_title,
                "affected_summary": affected_summary,
                "non_affected_summary": non_affected_summary,
                "article_body": article_text,
                "elapsed_time": start_time.elapsed().as_secs_f64(),
            });

            // Save the validated analysis back to the database
            if let Err(e) = db
                .add_article(
                    &article_url,
                    true,
                    None,
                    Some(&detailed_response_json.to_string()),
                    None,
                    Some(&article_hash),
                    Some(&title_domain_hash),
                    None, // Placeholder for future updates
                )
                .await
            {
                error!(target: TARGET_LLM_REQUEST, "Failed to save article to database: {:?}", e);
                return false;
            }

            // Notify Slack
            send_to_slack(
                &format!("*<{}|{}>*", article_url, article_title),
                &detailed_response_json.to_string(),
                slack_token,
                slack_channel,
            )
            .await;

            // Item processed successfully
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
                        // Collect database statistics
                        let stats = match db.collect_stats().await {
                            Ok(stats) => stats,
                            Err(e) => {
                                error!(target: TARGET_LLM_REQUEST, "Failed to collect database stats: {:?}", e);
                                String::from("N/A")
                            }
                        };

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
                            "model": llm_params.model,
                            "stats": stats
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
                        if let Some(r2_url) = send_to_app(&response_json).await {
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
    places: BTreeMap<std::string::String, BTreeMap<std::string::String, Vec<std::string::String>>>,
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
                places: places,
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
    debug!("Starting analysis for article: {}", article_url);

    // Re-summarize the article with the analysis worker.
    let summary_prompt = prompts::summary_prompt(article_text);
    debug!("Generated summary prompt: {:?}", summary_prompt);
    let summary = generate_llm_response(&summary_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_else(|| {
            warn!("Failed to generate summary");
            String::new()
        });
    info!("Generated summary: {:?}", summary);

    // Now perform the rest of the analysis.
    let tiny_summary_prompt = prompts::tiny_summary_prompt(&summary);
    let tiny_title_prompt = prompts::tiny_title_prompt(&summary);
    let critical_analysis_prompt = prompts::critical_analysis_prompt(article_text);
    let logical_fallacies_prompt = prompts::logical_fallacies_prompt(article_text);
    let source_analysis_prompt = prompts::source_analysis_prompt(article_html, article_url);

    debug!("Generated tiny summary prompt: {:?}", tiny_summary_prompt);
    let tiny_summary = generate_llm_response(&tiny_summary_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_else(|| {
            warn!("Failed to generate tiny summary");
            String::new()
        });
    info!("Generated tiny summary: {:?}", tiny_summary);

    debug!("Generated tiny title prompt: {:?}", tiny_title_prompt);
    let tiny_title = generate_llm_response(&tiny_title_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_else(|| {
            warn!("Failed to generate tiny title");
            String::new()
        });
    info!("Generated tiny title: {:?}", tiny_title);

    debug!(
        "Generated critical analysis prompt: {:?}",
        critical_analysis_prompt
    );
    let critical_analysis =
        generate_llm_response(&critical_analysis_prompt, llm_params, worker_detail)
            .await
            .unwrap_or_else(|| {
                warn!("Failed to generate critical analysis");
                String::new()
            });
    info!("Generated critical analysis: {:?}", critical_analysis);

    debug!(
        "Generated logical fallacies prompt: {:?}",
        logical_fallacies_prompt
    );
    let logical_fallacies =
        generate_llm_response(&logical_fallacies_prompt, llm_params, worker_detail)
            .await
            .unwrap_or_else(|| {
                warn!("Failed to generate logical fallacies");
                String::new()
            });
    info!("Generated logical fallacies: {:?}", logical_fallacies);

    debug!(
        "Generated source analysis prompt: {:?}",
        source_analysis_prompt
    );
    let source_analysis = generate_llm_response(&source_analysis_prompt, llm_params, worker_detail)
        .await
        .unwrap_or_else(|| {
            warn!("Failed to generate source analysis");
            String::new()
        });
    info!("Generated source analysis: {:?}", source_analysis);

    let relation_response = if let Some(topic) = topic {
        let relation_prompt = prompts::relation_to_topic_prompt(article_text, topic);
        debug!("Generated relation to topic prompt: {:?}", relation_prompt);
        generate_llm_response(&relation_prompt, llm_params, worker_detail)
            .await
            .or_else(|| {
                warn!("Failed to generate relation to topic");
                None
            })
    } else {
        None
    };
    info!("Generated relation response: {:?}", relation_response);

    info!("Completed analysis for article: {}", article_url);

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
