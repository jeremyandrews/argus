use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::app::util::send_to_app;
use crate::db::Database;
use crate::decision_worker::FeedItem;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::slack::send_to_slack;
use crate::util::{parse_places_data_detailed, parse_places_data_hierarchical};
use crate::vector::{get_article_vectors, get_similar_articles, store_embedding};
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

/// Converts sources_quality and argument_quality (values 1-3) into a combined quality score
/// where 1 = -1, 2 = 1, 3 = 2 points.
///
/// # Arguments
/// * `sources_quality` - Rating of sources quality from 1-3
/// * `argument_quality` - Rating of argument quality from 1-3
///
/// # Returns
/// * `i8` - Combined quality score ranging from -2 to 4
pub fn calculate_quality_score(sources_quality: u8, argument_quality: u8) -> i8 {
    // Transform values: 1 -> -1, 2 -> 1, 3 -> 2
    let sources_score = match sources_quality {
        1 => -1,
        2 => 1,
        3 => 2,
        _ => 0, // Default for invalid values
    };

    let argument_score = match argument_quality {
        1 => -1,
        2 => 1,
        3 => 2,
        _ => 0, // Default for invalid values
    };

    // Combine scores
    sources_score + argument_score
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
            article_html,
            article_hash,
            title_domain_hash,
            threat_regions,
            pub_date,
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
            info!("flat threat_regions: {:?}", threat_regions);

            // Parse the JSON threat_regions
            let threat_regions: serde_json::Value = serde_json::from_str(&threat_regions)
                .unwrap_or_else(|_| json!({"impacted_regions": []}));
            info!("json threat_regions: {:?}", threat_regions);

            let mut directly_affected_people: BTreeMap<String, HashSet<String>> = BTreeMap::new();
            let mut indirectly_affected_people: BTreeMap<String, HashSet<String>> = BTreeMap::new();

            // Iterate through the threat regions
            if let Some(impacted_regions) = threat_regions["impacted_regions"].as_array() {
                for region in impacted_regions {
                    let continent = region["continent"].as_str().unwrap_or("");
                    let country = region["country"].as_str().unwrap_or("");
                    let region_name = region["region"].as_str().unwrap_or("");
                    info!(
                        "url: {} checking continent: {}, country: {}, region_name: {}",
                        article_url, continent, country, region_name
                    );

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
                                info!("region_prompt: {}", region_prompt);
                                let region_response = generate_llm_response(
                                    &region_prompt,
                                    &llm_params,
                                    worker_detail,
                                )
                                .await
                                .unwrap_or_default();

                                // Parse the response for yes/no
                                if region_response.trim().to_lowercase().starts_with("yes") {
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
                                            for person in people {
                                                let parts: Vec<&str> = person.split(", ").collect();
                                                if parts.len() >= 3 {
                                                    let name = parts[0].to_string();
                                                    let city = parts[2].to_string();
                                                    directly_affected_people
                                                        .entry(city.clone())
                                                        .or_insert_with(HashSet::new)
                                                        .insert(name);
                                                }
                                            }
                                        } else {
                                            for person in people {
                                                let parts: Vec<&str> = person.split(", ").collect();
                                                if parts.len() >= 3 {
                                                    let name = parts[0].to_string();
                                                    let city = parts[2].to_string();
                                                    indirectly_affected_people
                                                        .entry(city.clone())
                                                        .or_insert_with(HashSet::new)
                                                        .insert(name);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let affected_summary = if !directly_affected_people.is_empty() {
                let mut summary =
                    String::from("This article directly affects people in these locations: ");
                let mut city_summaries = Vec::new();
                for (city, names) in directly_affected_people.iter() {
                    let mut sorted_names: Vec<String> = names.iter().cloned().collect();
                    sorted_names.sort();
                    let names_str = sorted_names.join(", ");
                    city_summaries.push(format!("{} ({})", city, names_str));
                }
                summary.push_str(&city_summaries.join("; "));
                summary.push('.');
                summary
            } else {
                String::new()
            };

            let non_affected_summary = if !indirectly_affected_people.is_empty() {
                let mut summary =
                    String::from("This article indirectly affects people in these locations: ");
                let mut city_summaries = Vec::new();
                for (city, names) in indirectly_affected_people.iter() {
                    let mut sorted_names: Vec<String> = names.iter().cloned().collect();
                    sorted_names.sort();
                    let names_str = sorted_names.join(", ");
                    city_summaries.push(format!("{} ({})", city, names_str));
                }
                summary.push_str(&city_summaries.join("; "));
                summary.push('.');
                summary
            } else {
                String::new()
            };

            if !affected_summary.is_empty() || !non_affected_summary.is_empty() {
                info!(
                    "article_url: {}, affected_summary({}) non_affected_summary({})",
                    article_url, affected_summary, non_affected_summary
                );

                // Determine how it does or does not affect.
                let how_does_it_affect = if !affected_summary.is_empty() {
                    let how_does_it_affect_prompt =
                        prompts::how_does_it_affect_prompt(&article_text, &affected_summary);
                    debug!(
                        "Generated how_does_it_affect prompt: {:?}",
                        how_does_it_affect_prompt
                    );
                    generate_llm_response(&how_does_it_affect_prompt, llm_params, worker_detail)
                        .await
                        .unwrap_or_else(|| {
                            warn!("Failed to generate how_does_it_affect");
                            String::new()
                        })
                } else {
                    String::new()
                };
                let why_not_affect = if !non_affected_summary.is_empty() {
                    let why_not_affect_prompt =
                        prompts::why_not_affect_prompt(&article_text, &non_affected_summary);
                    debug!(
                        "Generated why_not_affect prompt: {:?}",
                        why_not_affect_prompt
                    );
                    generate_llm_response(&why_not_affect_prompt, llm_params, worker_detail)
                        .await
                        .unwrap_or_else(|| {
                            warn!("Failed to generate why_not_affect");
                            String::new()
                        })
                } else {
                    String::new()
                };

                // Determine the topic based on the match type
                let topic = if !affected_summary.is_empty() {
                    "Alert: Direct"
                } else {
                    "Alert: Near"
                };

                // Construct relation_to_topic
                let relation_to_topic = if !affected_summary.is_empty()
                    && !non_affected_summary.is_empty()
                {
                    format!(
                        "{}\n\n{}\n\n{}\n\n{}",
                        affected_summary, how_does_it_affect, non_affected_summary, why_not_affect
                    )
                } else if !affected_summary.is_empty() {
                    format!("{}\n\n{}", affected_summary, how_does_it_affect)
                } else {
                    format!("{}\n\n{}", non_affected_summary, why_not_affect)
                };

                // Determine if there's an affected hint to share.
                let affected = if !affected_summary.is_empty() {
                    affected_summary.clone()
                } else {
                    String::new()
                };

                let (
                    summary,
                    tiny_summary,
                    tiny_title,
                    critical_analysis,
                    logical_fallacies,
                    source_analysis,
                    _relation,
                    sources_quality,
                    argument_quality,
                    source_type,
                    additional_insights,
                ) = process_analysis(
                    &article_text,
                    &article_html,
                    &article_url,
                    None, // No specific topic for life safety items
                    pub_date.as_deref(),
                    llm_params,
                    worker_detail,
                )
                .await;

                // Collect database statistics
                let stats = match db.collect_stats().await {
                    Ok(stats) => stats,
                    Err(e) => {
                        error!(target: TARGET_LLM_REQUEST, "Failed to collect database stats: {:?}", e);
                        String::from("N/A")
                    }
                };

                let quality = calculate_quality_score(sources_quality, argument_quality);

                // Construct the response JSON using the results from process_analysis
                let mut response_json = json!({
                    "topic": topic,
                    "title": article_title,
                    "url": article_url,
                    "article_body": article_text,
                    "pub_date": pub_date,
                    "tiny_summary": tiny_summary,
                    "tiny_title": tiny_title,
                    "summary": summary,
                    "affected": affected,
                    "critical_analysis": critical_analysis,
                    "logical_fallacies": logical_fallacies,
                    "relation_to_topic": relation_to_topic,
                    "source_analysis": source_analysis,
                    "additional_insights": additional_insights,
                    "sources_quality": sources_quality,
                    "argument_quality": argument_quality,
                    "quality": quality,
                    "source_type": source_type,
                    "elapsed_time": start_time.elapsed().as_secs_f64(),
                    "model": llm_params.model,
                    "stats": stats
                });

                // Save the article first
                let article_id = match db
                    .add_article(
                        &article_url,
                        true,
                        Some(topic),
                        Some(&response_json.to_string()),
                        Some(&tiny_summary),
                        Some(&article_hash),
                        Some(&title_domain_hash),
                        None, // Placeholder for R2 URL, will update later
                        pub_date.as_deref(),
                    )
                    .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        error!(
                            target: TARGET_LLM_REQUEST,
                            "Failed to save article to database: {:?}", e
                        );
                        return false; // Skip processing if saving fails
                    }
                };

                // Generate vector embedding
                let vector_start = Instant::now();
                if let Ok(Some(embedding)) = get_article_vectors(&article_text).await {
                    info!(
                        "Generated vector embedding with {} dimensions in {:?}",
                        embedding.len(),
                        vector_start.elapsed()
                    );
                    if let Ok(similar_articles) = get_similar_articles(&embedding, 10).await {
                        let mut similar_articles_with_details = Vec::new();
                        for article in similar_articles {
                            if let Ok(Some((url, title, tiny_summary))) =
                                db.get_article_details_by_id(article.id).await
                            {
                                similar_articles_with_details.push(json!({
                                    "id": article.id,
                                    "url": url,
                                    "title": title.unwrap_or_else(|| "Unknown Title".to_string()),
                                    "tiny_summary": tiny_summary,
                                    "category": article.category,
                                    "published_date": article.published_date,
                                    "quality_score": article.quality_score,
                                    "similarity_score": article.score
                                }));
                            } else {
                                // Include basic info if details can't be fetched
                                similar_articles_with_details.push(json!({
                                    "id": article.id,
                                    "category": article.category,
                                    "published_date": article.published_date,
                                    "quality_score": article.quality_score,
                                    "similarity_score": article.score
                                }));
                            }
                        }
                        response_json["similar_articles"] = json!(similar_articles_with_details);
                    }
                    if let Err(e) = store_embedding(
                        article_id,
                        &embedding,
                        pub_date.as_deref().unwrap_or("unknown"),
                        topic,
                        quality,
                    )
                    .await
                    {
                        error!(
                            target: TARGET_LLM_REQUEST,
                            "Failed to store vector embedding: {:?}", e
                        );
                    }
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

                // Notify Slack
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
            }

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
                    pub_date,
                ))) => {
                    let mut llm_params_clone = llm_params.clone();

                    let start_time = std::time::Instant::now();

                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from matched topics queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

                    if db.has_hash(&article_hash).await.unwrap_or(false)
                        || db
                            .has_title_domain_hash(&title_domain_hash)
                            .await
                            .unwrap_or(false)
                    {
                        info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: already processed, skipping {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);
                        return false;
                    }

                    let (
                        summary,
                        tiny_summary,
                        tiny_title,
                        critical_analysis,
                        logical_fallacies,
                        source_analysis,
                        relation,
                        sources_quality,
                        argument_quality,
                        source_type,
                        additional_insights,
                    ) = process_analysis(
                        &article_text,
                        &article_html,
                        &article_url,
                        Some(&topic),
                        pub_date.as_deref(),
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

                        let quality = calculate_quality_score(sources_quality, argument_quality);

                        let mut response_json = json!({
                            "topic": topic,
                            "title": article_title,
                            "url": article_url,
                            "article_body": article_text,
                            "pub_date": pub_date,
                            "tiny_summary": tiny_summary,
                            "tiny_title": tiny_title,
                            "summary": summary,
                            "critical_analysis": critical_analysis,
                            "logical_fallacies": logical_fallacies,
                            "relation_to_topic": relation,
                            "source_analysis": source_analysis,
                            "additional_insights": additional_insights,
                            "sources_quality": sources_quality,
                            "argument_quality": argument_quality,
                            "quality": quality,
                            "source_type": source_type,
                            "elapsed_time": start_time.elapsed().as_secs_f64(),
                            "model": llm_params.model,
                            "stats": stats
                        });

                        // Save the article first
                        let article_id = match db
                            .add_article(
                                &article_url,
                                true,
                                Some(&topic),
                                Some(&response_json.to_string()),
                                Some(&tiny_summary),
                                Some(&article_hash),
                                Some(&title_domain_hash),
                                None, // Placeholder for R2 URL, will update later
                                pub_date.as_deref(),
                            )
                            .await
                        {
                            Ok(id) => id,
                            Err(e) => {
                                error!(
                                    target: TARGET_LLM_REQUEST,
                                    "Failed to save article to database: {:?}", e
                                );
                                return false; // Skip processing if saving fails
                            }
                        };

                        // Generate vector embedding
                        let vector_start = Instant::now();
                        if let Ok(Some(embedding)) = get_article_vectors(&article_text).await {
                            info!(
                                "Generated vector embedding with {} dimensions in {:?}",
                                embedding.len(),
                                vector_start.elapsed()
                            );
                            if let Ok(similar_articles) = get_similar_articles(&embedding, 10).await
                            {
                                let mut similar_articles_with_details = Vec::new();
                                for article in similar_articles {
                                    if let Ok(Some((url, title, tiny_summary))) =
                                        db.get_article_details_by_id(article.id).await
                                    {
                                        similar_articles_with_details.push(json!({
                                            "id": article.id,
                                            "url": url,
                                            "title": title.unwrap_or_else(|| "Unknown Title".to_string()),
                                            "tiny_summary": tiny_summary,
                                            "category": article.category,
                                            "published_date": article.published_date,
                                            "quality_score": article.quality_score,
                                            "similarity_score": article.score
                                        }));
                                    } else {
                                        // Include basic info if details can't be fetched
                                        similar_articles_with_details.push(json!({
                                            "id": article.id,
                                            "category": article.category,
                                            "published_date": article.published_date,
                                            "quality_score": article.quality_score,
                                            "similarity_score": article.score
                                        }));
                                    }
                                }
                                response_json["similar_articles"] =
                                    json!(similar_articles_with_details);
                            }
                            if let Err(e) = store_embedding(
                                article_id,
                                &embedding,
                                pub_date.as_deref().unwrap_or("default value"),
                                &topic,
                                quality,
                            )
                            .await
                            {
                                error!(
                                    target: TARGET_LLM_REQUEST,
                                    "Failed to store vector embedding: {:?}", e
                                );
                            }
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
    match db.fetch_and_delete_url_from_rss_queue("random").await {
        Ok(Some((url, title, pub_date))) => {
            if url.trim().is_empty() {
                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping empty URL in RSS queue.", worker_detail.name, worker_detail.id, worker_detail.model);
                return;
            }

            // Parse pub_date and check if the article is older than 3 days
            let is_old_article = if let Some(date_str) = &pub_date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .ok()
                    .map(|date| {
                        Utc::now().date_naive().signed_duration_since(date)
                            > ChronoDuration::days(3)
                    }) // Use ChronoDuration here
                    .unwrap_or(false)
            } else {
                false
            };

            if is_old_article {
                info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping old article (published on {:?}): {}.", worker_detail.name, worker_detail.id, worker_detail.model, pub_date, url);

                // Store the old article in the database so we don’t process it again
                let _ = db
                    .add_article(
                        &url,
                        false, // Not relevant
                        None,  // No category
                        None,  // No analysis
                        None,  // No tiny summary
                        None,  // No hash
                        None,  // No title_domain_hash
                        None,  // No R2 URL
                        pub_date.as_deref(),
                    )
                    .await;
                return;
            }

            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: new URL: {} ({:?}).", worker_detail.name, worker_detail.id, worker_detail.model, url, title);

            let item = FeedItem {
                url: url.clone(),
                title,
                pub_date,
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

            // Use the same process_item logic from decision_worker
            crate::decision_worker::process_item(item, &mut params, &worker_detail).await;
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
}

/// Function to perform the analysis on an article.
/// Returns a tuple containing various analysis results.
async fn process_analysis(
    article_text: &str,
    article_html: &str,
    article_url: &str,
    topic: Option<&str>,
    pub_date: Option<&str>,
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
    u8,
    u8,
    String,
    String,
) {
    debug!("Starting analysis for article: {}", article_url);

    // First, verify we have content to analyze
    if article_text.trim().is_empty() {
        warn!("Empty article text, cannot perform analysis");
        return (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            None,
            2,
            2,
            String::from("none"),
            String::new(),
        );
    }

    // Start with summary to establish base understanding
    let summary_prompt = prompts::summary_prompt(article_text, pub_date);
    let summary = match generate_llm_response(&summary_prompt, llm_params, worker_detail).await {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            warn!("Failed to generate valid summary");
            return (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                None,
                2,
                2,
                String::from("none"),
                String::new(),
            );
        }
    };

    // Only proceed with other analyses if we have a valid summary
    let tiny_summary = generate_llm_response(
        &prompts::tiny_summary_prompt(&summary),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    let tiny_title = generate_llm_response(
        &prompts::tiny_title_prompt(&summary),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Critical analysis and logical fallacies need the full article text
    let critical_analysis = generate_llm_response(
        &prompts::critical_analysis_prompt(article_text, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    let logical_fallacies = generate_llm_response(
        &prompts::logical_fallacies_prompt(article_text, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Source analysis needs HTML and URL
    let source_analysis = generate_llm_response(
        &prompts::source_analysis_prompt(article_html, article_url, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Quality scores should only be generated if we have valid analyses
    let sources_quality = if !critical_analysis.is_empty() {
        generate_llm_response(
            &prompts::sources_quality_prompt(&critical_analysis),
            llm_params,
            worker_detail,
        )
        .await
        .and_then(|resp| resp.trim().parse::<u8>().ok())
        .unwrap_or(2)
    } else {
        2
    };

    let argument_quality = if !logical_fallacies.is_empty() {
        generate_llm_response(
            &prompts::argument_quality_prompt(&logical_fallacies),
            llm_params,
            worker_detail,
        )
        .await
        .and_then(|resp| resp.trim().parse::<u8>().ok())
        .unwrap_or(2)
    } else {
        2
    };

    // Source type should only be generated if we have valid source analysis
    let source_type = if !source_analysis.is_empty() {
        generate_llm_response(
            &prompts::source_type_prompt(&source_analysis, article_url),
            llm_params,
            worker_detail,
        )
        .await
        .unwrap_or_else(|| String::from("none"))
        .trim()
        .to_string()
    } else {
        String::from("none")
    };

    // Topic relation is optional and should only be generated if we have a topic
    let relation_response = if let Some(topic) = topic {
        generate_llm_response(
            &prompts::relation_to_topic_prompt(article_text, topic, pub_date),
            llm_params,
            worker_detail,
        )
        .await
    } else {
        None
    };

    // Generate additional insights after other analyses are complete
    let additional_insights = if !summary.is_empty() && !critical_analysis.is_empty() {
        generate_llm_response(
            &prompts::additional_insights_prompt(article_text, pub_date),
            llm_params,
            worker_detail,
        )
        .await
        .unwrap_or_default()
    } else {
        String::new()
    };
    (
        summary,
        tiny_summary,
        tiny_title,
        critical_analysis,
        logical_fallacies,
        source_analysis,
        relation_response,
        sources_quality,
        argument_quality,
        source_type,
        additional_insights,
    )
}
