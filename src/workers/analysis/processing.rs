use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::app::util::send_to_app;
use crate::db::core::Database;
use crate::llm::generate_text_response;
use crate::prompt;
use crate::slack::send_to_slack;
use crate::workers::common::calculate_quality_score;
use crate::{TextLLMParams, WorkerDetail, TARGET_LLM_REQUEST};

use super::quality::process_analysis;
use super::similarity::process_article_similarity;

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
) -> bool {
    // First, try to process an item from the life safety queue
    if let Ok(Some((
        article_url,
        article_title,
        article_text,
        article_html,
        article_hash,
        title_domain_hash,
        threat_regions,
        pub_date,
    ))) = db.fetch_and_delete_from_life_safety_queue().await
    {
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
        )
        .await;

        return true;
    }

    // If no life safety item, try to process an item from the matched topics queue
    if let Ok(Some((
        article_text,
        article_html,
        article_url,
        article_title,
        article_hash,
        title_domain_hash,
        topic,
        pub_date,
    ))) = db.fetch_and_delete_from_matched_topics_queue().await
    {
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
        )
        .await;

        if success {
            return true;
        }
    } else {
        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Matched Topics queue empty, sleeping 10 seconds...", worker_detail.name, worker_detail.id, worker_detail.model);
        sleep(Duration::from_secs(10)).await;
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
) -> bool {
    let start_time = Instant::now();
    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: pulled from life safety queue {}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url);

    // Check if article was already processed
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
            "action_recommendations": action_recommendations,
            "talking_points": talking_points,
            "sources_quality": sources_quality,
            "argument_quality": argument_quality,
            "quality": quality,
            "source_type": source_type,
            "elapsed_time": start_time.elapsed().as_secs_f64(),
            "model": llm_params.base.model.clone(),
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
                None, // event_date
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

        // Process vector embeddings and entities
        if let Err(e) = process_article_similarity(
            db,
            article_id,
            &summary,
            &article_text,
            pub_date.as_deref(),
            &article_hash,
            &title_domain_hash,
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
                "Failed to process article similarity: {:?}", e
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
) -> bool {
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
        action_recommendations,
        talking_points,
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
            "action_recommendations": action_recommendations,
            "talking_points": talking_points,
            "sources_quality": sources_quality,
            "argument_quality": argument_quality,
            "quality": quality,
            "source_type": source_type,
            "elapsed_time": start_time.elapsed().as_secs_f64(),
            "model": llm_params.base.model.clone(),
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
                None, // event_date
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

        // Process vector embeddings and entities
        if let Err(e) = process_article_similarity(
            db,
            article_id,
            &summary,
            &article_text,
            pub_date.as_deref(),
            &article_hash,
            &title_domain_hash,
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
                "Failed to process article similarity: {:?}", e
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
