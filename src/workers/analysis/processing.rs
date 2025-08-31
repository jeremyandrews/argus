use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use tokio::time::{sleep, timeout, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::app::util::send_to_app;
use crate::db::core::Database;
use crate::llm::generate_text_response;
use crate::prompt;
use crate::rate_limiter::OpenAIRateLimiter;
use crate::slack::send_to_slack;
use crate::workers::common::calculate_quality_score;
use crate::{TextLLMParams, WorkerDetail, TARGET_LLM_REQUEST};

use super::quality::process_analysis;

/// Function to process a single analysis item.
/// Returns true if an item was processed, false otherwise.
pub async fn process_analysis_item(
    worker_detail: &WorkerDetail,
    llm_params: &mut TextLLMParams,
    db: &Database,
    slack_token: &str,
    slack_channel: &str,
    places_detailed: &BTreeMap<
        String,
        BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>,
    >,
    rate_limiter: Option<&OpenAIRateLimiter>,
) -> bool {
    // First, try to process an item from the life safety queue
    match timeout(
        Duration::from_secs(30),
        db.fetch_and_delete_from_life_safety_queue(),
    )
    .await
    {
        Ok(Ok(Some((
            article_url,
            article_title,
            article_text,
            article_html,
            article_hash,
            title_domain_hash,
            threat_regions,
            pub_date,
        )))) => {
            process_life_safety_item(
                worker_detail,
                llm_params,
                db,
                slack_token,
                slack_channel,
                places_detailed,
                article_url,
                article_title,
                article_text,
                article_html,
                article_hash,
                title_domain_hash,
                threat_regions,
                pub_date,
                rate_limiter,
            )
            .await;

            return true;
        }
        Ok(Ok(None)) => {
            // No life safety items, continue to matched topics
        }
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error fetching life safety queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 30s: fetch_and_delete_from_life_safety_queue", worker_detail.name, worker_detail.id, worker_detail.model);
        }
    }

    // Try to process an item from the matched topics queue
    match timeout(
        Duration::from_secs(30),
        db.fetch_and_delete_from_matched_topics_queue(),
    )
    .await
    {
        Ok(Ok(Some((
            article_text,
            article_html,
            article_url,
            article_title,
            article_hash,
            title_domain_hash,
            topic,
            pub_date,
        )))) => {
            let success = process_matched_topic_item(
                worker_detail,
                llm_params,
                db,
                slack_token,
                slack_channel,
                article_text,
                article_html,
                article_url,
                article_title,
                article_hash,
                title_domain_hash,
                topic,
                pub_date,
                rate_limiter,
            )
            .await;

            if success {
                return true;
            }
        }
        Ok(Ok(None)) => {
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Matched Topics queue empty, sleeping 10 seconds...", worker_detail.name, worker_detail.id, worker_detail.model);
            sleep(Duration::from_secs(10)).await;
        }
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error fetching matched topics queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 30s: fetch_and_delete_from_matched_topics_queue", worker_detail.name, worker_detail.id, worker_detail.model);
        }
    }

    // If we reach here, no item was processed successfully
    false
}

/// Process an item from the life safety queue
async fn process_life_safety_item(
    worker_detail: &WorkerDetail,
    llm_params: &mut TextLLMParams,
    db: &Database,
    slack_token: &str,
    slack_channel: &str,
    places_detailed: &BTreeMap<
        String,
        BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>,
    >,
    article_url: String,
    article_title: String,
    article_text: String,
    article_html: String,
    article_hash: String,
    title_domain_hash: String,
    threat_regions: String,
    pub_date: Option<String>,
    rate_limiter: Option<&OpenAIRateLimiter>,
) -> bool {
    let start_time = Instant::now();
    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from life safety queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

    // Check if article was already processed
    let hash_exists = match timeout(Duration::from_secs(10), db.has_hash(&article_hash)).await {
        Ok(Ok(exists)) => exists,
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error checking hash: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            false
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 10s: has_hash", worker_detail.name, worker_detail.id, worker_detail.model);
            false
        }
    };

    let title_domain_hash_exists = match timeout(
        Duration::from_secs(10),
        db.has_title_domain_hash(&title_domain_hash),
    )
    .await
    {
        Ok(Ok(exists)) => exists,
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error checking title_domain_hash: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            false
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 10s: has_title_domain_hash", worker_detail.name, worker_detail.id, worker_detail.model);
            false
        }
    };

    if hash_exists || title_domain_hash_exists {
        info!(
            target: TARGET_LLM_REQUEST,
            "Article with hash {} or title_domain_hash {} was already processed. Skipping.",
            article_hash, title_domain_hash
        );
        return false;
    }
    info!("flat threat_regions: {:?}", threat_regions);

    // Parse the JSON threat_regions
    let threat_regions: serde_json::Value =
        serde_json::from_str(&threat_regions).unwrap_or_else(|_| json!({"impacted_regions": []}));
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
                        let region_prompt = prompt::region_threat_prompt(
                            &article_text,
                            region_name,
                            country,
                            continent,
                        );
                        info!("region_prompt: {}", region_prompt);
                        let region_response =
                            generate_text_response(&region_prompt, &llm_params, worker_detail)
                                .await
                                .unwrap_or_default();

                        // Parse the response for yes/no
                        if region_response.trim().to_lowercase().starts_with("yes") {
                            for (city_name, people) in cities.iter() {
                                let city_prompt = prompt::city_threat_prompt(
                                    &article_text,
                                    city_name,
                                    region_name,
                                    country,
                                    continent,
                                );
                                let city_response =
                                    generate_text_response(&city_prompt, llm_params, worker_detail)
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

    let affected_summary = build_affected_summary(&directly_affected_people);
    let non_affected_summary = build_affected_summary_indirect(&indirectly_affected_people);

    if !affected_summary.is_empty() || !non_affected_summary.is_empty() {
        info!(
            "article_url: {}, affected_summary({}) non_affected_summary({})",
            article_url, affected_summary, non_affected_summary
        );

        // Determine how it does or does not affect.
        let how_does_it_affect = if !affected_summary.is_empty() {
            let how_does_it_affect_prompt =
                prompt::how_does_it_affect_prompt(&article_text, &affected_summary);
            debug!(
                "Generated how_does_it_affect prompt: {:?}",
                how_does_it_affect_prompt
            );
            generate_text_response(&how_does_it_affect_prompt, &llm_params, worker_detail)
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
                prompt::why_not_affect_prompt(&article_text, &non_affected_summary);
            debug!(
                "Generated why_not_affect prompt: {:?}",
                why_not_affect_prompt
            );
            generate_text_response(&why_not_affect_prompt, &llm_params, worker_detail)
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
        let relation_to_topic = if !affected_summary.is_empty() && !non_affected_summary.is_empty()
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

        let analysis_result = process_analysis(
            &article_text,
            &article_html,
            &article_url,
            None, // No specific topic for life safety items
            pub_date.as_deref(),
            llm_params,
            worker_detail,
            rate_limiter,
        )
        .await;

        // Check if analysis failed due to empty results (likely rate limiting or other errors)
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
            action_recommendations,
            talking_points,
            eli5,
        ) = analysis_result;

        if summary.is_empty()
            || tiny_summary.is_empty()
            || critical_analysis.is_empty()
            || logical_fallacies.is_empty()
        {
            // Analysis failed - put the item back into the life safety queue for retry
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Analysis failed for {}, returning to life safety queue for retry", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

            // Convert threat_regions back to string for database storage
            let threat_regions_str = threat_regions.to_string();
            if let Err(e) = db
                .add_to_life_safety_queue(
                    &threat_regions_str,
                    &article_url,
                    &article_title,
                    &article_text,
                    &article_html,
                    &article_hash,
                    &title_domain_hash,
                    pub_date.as_deref(),
                )
                .await
            {
                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Failed to return item to life safety queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            }
            return false;
        }

        // Collect database statistics
        let stats = match db.collect_runtime_stats().await {
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
            "action_recommendations": action_recommendations,
            "talking_points": talking_points,
            "eli5": eli5,
            "sources_quality": sources_quality,
            "argument_quality": argument_quality,
            "quality": quality,
            "source_type": source_type,
            "elapsed_time": start_time.elapsed().as_secs_f64(),
            "model": llm_params.base.model.clone(),
            "stats": stats
        });

        // Save the article first with timeout
        let article_id = match timeout(
            Duration::from_secs(30),
            db.add_article(
                &article_url,
                true,
                Some(topic),
                Some(&response_json.to_string()),
                Some(&tiny_summary),
                Some(&article_hash),
                Some(&title_domain_hash),
                None, // Placeholder for R2 URL, will update later
                pub_date.as_deref(),
                None, // event_date
            ),
        )
        .await
        {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Failed to save article to database: {:?}",
                    worker_detail.name, worker_detail.id, worker_detail.model, e
                );
                return false; // Skip processing if saving fails
            }
            Err(_) => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: [TIMEOUT] Database operation timed out after 30s: add_article (life_safety_item)",
                    worker_detail.name, worker_detail.id, worker_detail.model
                );
                return false;
            }
        };

        // Process vector embeddings, entities, and clustering (inline)
        if let Err(e) = process_similarity_and_clustering_inline(
            db,
            article_id,
            &summary,
            &article_text,
            pub_date.as_deref(),
            Some(topic),
            quality,
            &mut response_json,
            llm_params,
            worker_detail,
        )
        .await
        {
            error!(
                target: TARGET_LLM_REQUEST,
                "Failed to process similarity and clustering: {:?}", e
            );
        }

        // Add the article ID to the JSON now that we have it
        response_json["id"] = json!(article_id);

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

        return true;
    }

    false
}

/// Process an item from the matched topics queue
async fn process_matched_topic_item(
    worker_detail: &WorkerDetail,
    llm_params: &mut TextLLMParams,
    db: &Database,
    slack_token: &str,
    slack_channel: &str,
    article_text: String,
    article_html: String,
    article_url: String,
    article_title: String,
    article_hash: String,
    title_domain_hash: String,
    topic: String,
    pub_date: Option<String>,
    rate_limiter: Option<&OpenAIRateLimiter>,
) -> bool {
    let mut llm_params_clone = llm_params.clone();

    let start_time = std::time::Instant::now();

    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from matched topics queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

    // Check if article was already processed with timeouts
    let hash_exists = match timeout(Duration::from_secs(10), db.has_hash(&article_hash)).await {
        Ok(Ok(exists)) => exists,
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error checking hash: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            false
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 10s: has_hash", worker_detail.name, worker_detail.id, worker_detail.model);
            false
        }
    };

    let title_domain_hash_exists = match timeout(
        Duration::from_secs(10),
        db.has_title_domain_hash(&title_domain_hash),
    )
    .await
    {
        Ok(Ok(exists)) => exists,
        Ok(Err(e)) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Database error checking title_domain_hash: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            false
        }
        Err(_) => {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: [TIMEOUT] Database operation timed out after 10s: has_title_domain_hash", worker_detail.name, worker_detail.id, worker_detail.model);
            false
        }
    };

    if hash_exists || title_domain_hash_exists {
        info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: already processed, skipping {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);
        return false;
    }

    let analysis_result = process_analysis(
        &article_text,
        &article_html,
        &article_url,
        Some(&topic),
        pub_date.as_deref(),
        &mut llm_params_clone,
        worker_detail,
        rate_limiter,
    )
    .await;

    // Check if analysis failed due to empty results (likely rate limiting or other errors)
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
        action_recommendations,
        talking_points,
        eli5,
    ) = analysis_result;

    if summary.is_empty()
        || tiny_summary.is_empty()
        || critical_analysis.is_empty()
        || logical_fallacies.is_empty()
    {
        // Analysis failed - put the item back into the matched topics queue for retry
        error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Analysis failed for {}, returning to matched topics queue for retry", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

        if let Err(e) = db
            .add_to_matched_topics_queue(
                &article_text,
                &article_html,
                &article_url,
                &article_title,
                &article_hash,
                &title_domain_hash,
                &topic,
                pub_date.as_deref(),
            )
            .await
        {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Failed to return item to matched topics queue: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
        }
        return false;
    }

    // Analysis succeeded, continue with processing
    if !summary.is_empty()
        && !tiny_summary.is_empty()
        && !critical_analysis.is_empty()
        && !logical_fallacies.is_empty()
    {
        // Collect database statistics
        let stats = match db.collect_runtime_stats().await {
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
            "action_recommendations": action_recommendations,
            "talking_points": talking_points,
            "eli5": eli5,
            "sources_quality": sources_quality,
            "argument_quality": argument_quality,
            "quality": quality,
            "source_type": source_type,
            "elapsed_time": start_time.elapsed().as_secs_f64(),
            "model": llm_params.base.model.clone(),
            "stats": stats
        });

        // Save the article first with timeout
        let article_id = match timeout(
            Duration::from_secs(30),
            db.add_article(
                &article_url,
                true,
                Some(&topic),
                Some(&response_json.to_string()),
                Some(&tiny_summary),
                Some(&article_hash),
                Some(&title_domain_hash),
                None, // Placeholder for R2 URL, will update later
                pub_date.as_deref(),
                None, // event_date
            ),
        )
        .await
        {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Failed to save article to database: {:?}",
                    worker_detail.name, worker_detail.id, worker_detail.model, e
                );
                return false; // Skip processing if saving fails
            }
            Err(_) => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: [TIMEOUT] Database operation timed out after 30s: add_article (matched_topic_item)",
                    worker_detail.name, worker_detail.id, worker_detail.model
                );
                return false;
            }
        };

        // Process vector embeddings, entities, and clustering (inline)
        if let Err(e) = process_similarity_and_clustering_inline(
            db,
            article_id,
            &summary,
            &article_text,
            pub_date.as_deref(),
            Some(&topic),
            quality,
            &mut response_json,
            &mut llm_params_clone,
            worker_detail,
        )
        .await
        {
            error!(
                target: TARGET_LLM_REQUEST,
                "Failed to process similarity and clustering: {:?}", e
            );
        }

        // Add the article ID to the JSON now that we have it
        response_json["id"] = json!(article_id);

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
    }

    false
}

/// Build a summary string for directly affected people
fn build_affected_summary(directly_affected_people: &BTreeMap<String, HashSet<String>>) -> String {
    if !directly_affected_people.is_empty() {
        let mut summary = String::from("This article directly affects people in these locations: ");
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
    }
}

/// Build a summary string for indirectly affected people
fn build_affected_summary_indirect(
    indirectly_affected_people: &BTreeMap<String, HashSet<String>>,
) -> String {
    if !indirectly_affected_people.is_empty() {
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
    }
}

/// Inline processing of similarity search, entity extraction, and clustering
/// Replaces the old process_article_similarity function to fix timing issues
async fn process_similarity_and_clustering_inline(
    db: &Database,
    article_id: i64,
    summary: &str,
    article_text: &str,
    pub_date: Option<&str>,
    topic: Option<&str>,
    quality: i8,
    response_json: &mut serde_json::Value,
    llm_params: &mut TextLLMParams,
    worker_detail: &WorkerDetail,
) -> Result<(), anyhow::Error> {
    use crate::vector::{
        embedding::get_article_vectors, search::get_similar_articles_with_entities,
        storage::store_embedding,
    };
    use crate::JsonSchemaType;

    // Generate vector embedding
    let vector_start = Instant::now();
    if let Ok(Some(embedding)) = get_article_vectors(summary).await {
        info!(
            "Generated vector embedding with {} dimensions in {:?}",
            embedding.len(),
            vector_start.elapsed()
        );

        // Extract entities BEFORE similarity search
        let entity_extraction_start = Instant::now();
        let mut entity_ids: Option<Vec<i64>> = None;

        // Create JsonLLMParams for entity extraction
        let json_params = crate::JsonLLMParams {
            base: llm_params.base.clone(),
            schema_type: JsonSchemaType::EntityExtraction,
        };

        match crate::entity::extraction::extract_entities(
            article_text,
            pub_date,
            &json_params,
            worker_detail,
        )
        .await
        {
            Ok(extracted_entities) => {
                info!(
                    "Extracted {} entities in {:?}",
                    extracted_entities.entities.len(),
                    entity_extraction_start.elapsed()
                );

                // Increment entity extraction counter
                if let Err(e) = db
                    .increment_counter(
                        "entities_extracted_total",
                        extracted_entities.entities.len() as i32,
                    )
                    .await
                {
                    warn!(
                        "Failed to increment entities_extracted_total counter: {}",
                        e
                    );
                }

                // Add entities to response JSON
                response_json["entities"] = json!(extracted_entities.to_frontend_json_array());

                // Store entities and get IDs
                let entities_json =
                    serde_json::to_string(&extracted_entities).unwrap_or_else(|_| "{}".to_string());

                match timeout(
                    Duration::from_secs(30),
                    db.process_entity_extraction(article_id, &entities_json),
                )
                .await
                {
                    Ok(Ok(ids)) => {
                        info!(
                            "Successfully processed entity extraction for article {} with {} entities",
                            article_id, ids.len()
                        );
                        entity_ids = Some(ids);
                    }
                    Ok(Err(e)) => {
                        error!("Failed to process entity extraction: {:?}", e);
                    }
                    Err(_) => {
                        error!("[TIMEOUT] Database operation timed out after 30s: process_entity_extraction");
                    }
                }
            }
            Err(e) => {
                error!("Failed to extract entities: {:?}", e);
            }
        }

        // Get event date
        let (_, event_date) = db
            .get_article_details_with_dates(article_id)
            .await
            .unwrap_or((None, None));

        // Get similar articles using the unified algorithm
        let similar_articles = match get_similar_articles_with_entities(
            &embedding,
            10,
            entity_ids.as_deref(),
            event_date.as_deref(),
            Some(article_id),
        )
        .await
        {
            Ok(articles) => articles,
            Err(e) => {
                error!("Failed to get similar articles: {:?}", e);
                Vec::new()
            }
        };

        // Build similar articles JSON from results
        let mut similar_articles_with_details = Vec::new();
        for article in &similar_articles {
            if let Ok(Some((json_url, title, tiny_summary))) =
                db.get_article_details_by_id(article.id).await
            {
                similar_articles_with_details.push(build_similar_article_json(
                    article,
                    Some(json_url),
                    title,
                    Some(tiny_summary),
                ));
            } else {
                similar_articles_with_details
                    .push(build_similar_article_json(article, None, None, None));
            }
        }
        response_json["similar_articles"] = json!(similar_articles_with_details);

        // Use the SAME results for clustering - this is the key fix!
        let cluster_id = match crate::db::cluster::assign_article_to_cluster_from_similar(
            db,
            article_id,
            &similar_articles,
        )
        .await
        {
            Ok(id) => {
                info!("Assigned article {} to cluster {}", article_id, id);
                id
            }
            Err(e) => {
                error!("Failed to assign article {} to cluster: {}", article_id, e);
                0
            }
        };

        // Generate cluster summary if assigned to a cluster
        if cluster_id > 0 {
            // Extract current article data from response_json for cluster summary
            let current_article_data = crate::clustering::types::CurrentArticleData {
                id: article_id,
                title: response_json["title"].as_str().unwrap_or("").to_string(),
                tiny_title: response_json["tiny_title"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                url: response_json["url"].as_str().unwrap_or("").to_string(),
                tiny_summary: response_json["tiny_summary"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                quality_score: quality,
                pub_date: pub_date.map(|s| s.to_string()),
            };

            match crate::clustering::generate_cluster_summary(
                db,
                &llm_params,
                cluster_id,
                Some(current_article_data),
            )
            .await
            {
                Ok(summary) => {
                    info!(
                        "Generated summary for cluster {} (length: {})",
                        cluster_id,
                        summary.len()
                    );

                    // Update cluster significance
                    if let Ok(score) =
                        crate::clustering::calculate_cluster_significance(db, cluster_id).await
                    {
                        info!(
                            "Updated significance score for cluster {}: {:.4}",
                            cluster_id, score
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to generate summary for cluster {}: {}",
                        cluster_id, e
                    );
                }
            }

            // Check for potential cluster merges
            match crate::clustering::check_and_merge_similar_clusters(db, cluster_id, &llm_params)
                .await
            {
                Ok(Some(new_cluster_id)) => {
                    info!(
                        "Merged cluster {} into new cluster {}",
                        cluster_id, new_cluster_id
                    );
                }
                Ok(None) => {
                    debug!("No clusters merged for cluster {}", cluster_id);
                }
                Err(e) => {
                    error!("Error checking for cluster merges: {}", e);
                }
            }
        }

        // Store embedding
        if let Err(e) = store_embedding(
            article_id,
            &embedding,
            pub_date,
            topic,
            quality,
            entity_ids,
            event_date.as_deref(),
        )
        .await
        {
            error!("Failed to store vector embedding: {:?}", e);
        }

        // Add cluster summary to response JSON if article belongs to a cluster
        if let Ok(Some(cluster_summary)) =
            crate::db::cluster::get_article_cluster_summary(db, article_id).await
        {
            response_json["cluster_summary"] = serde_json::json!(cluster_summary);
        }
    }

    Ok(())
}

/// Converts an ArticleMatch and article details into a standardized JSON representation
fn build_similar_article_json(
    article: &crate::vector::types::ArticleMatch,
    json_url: Option<String>,
    title: Option<String>,
    tiny_summary: Option<String>,
) -> serde_json::Value {
    json!({
        // Basic fields
        "id": article.id,
        "json_url": json_url.unwrap_or_else(|| "Unknown URL".to_string()),
        "title": title.unwrap_or_else(|| "Unknown Title".to_string()),
        "tiny_summary": tiny_summary.unwrap_or_default(),
        "category": article.category.clone(),
        "published_date": article.published_date.clone(),
        "quality_score": article.quality_score,
        "similarity_score": article.score,

        // Vector quality fields - Explicitly unwrap Option types with defaults
        "vector_score": article.vector_score.unwrap_or(0.0),
        "vector_active_dimensions": article.vector_active_dimensions.unwrap_or(0),
        "vector_magnitude": article.vector_magnitude.unwrap_or(0.0),

        // Entity similarity fields - Explicitly unwrap Option types with defaults
        "entity_overlap_count": article.entity_overlap_count.unwrap_or(0),
        "primary_overlap_count": article.primary_overlap_count.unwrap_or(0),
        "person_overlap": article.person_overlap.unwrap_or(0.0),
        "org_overlap": article.org_overlap.unwrap_or(0.0),
        "location_overlap": article.location_overlap.unwrap_or(0.0),
        "event_overlap": article.event_overlap.unwrap_or(0.0),
        "temporal_proximity": article.temporal_proximity.unwrap_or(0.0),

        // Formula explanation
        "similarity_formula": article.similarity_formula.as_ref().map_or_else(|| "Unknown".to_string(), |s| s.clone())
    })
}
