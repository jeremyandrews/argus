use serde_json::json;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::slack::send_to_slack;
use crate::{LLMClient, LLMParams, TARGET_LLM_REQUEST};

pub async fn analysis_loop(
    worker_id: i16,
    llm_client: &LLMClient,
    model: &str,
    slack_token: &str,
    default_slack_channel: &str,
    temperature: f32,
) {
    let db = Database::instance().await;

    info!(target: TARGET_LLM_REQUEST, "Analysis worker {}: starting analysis_loop.", worker_id);
    debug!(
        "Analysis worker {} is running with model '{}' using {:?}.",
        worker_id, model, llm_client
    );

    loop {
        // Process items from the life safety queue first
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
                affected_places,
                non_affected_people,
                non_affected_places,
            ))) => {
                let start_time = std::time::Instant::now();

                debug!(
                    target: TARGET_LLM_REQUEST,
                    "worker {}: Pulled item from life safety queue: {}",
                    worker_id, article_url
                );

                // Check again if the article hash already exists in the database before reviewing with LLM.
                if db.has_hash(&article_hash).await.unwrap_or(false) {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Article with hash {} was already processed (second check).",
                        article_hash
                    );
                    return;
                }

                // Check again if the title_domain_hash already exists in the database before reviewing with LLM.
                if db
                    .has_title_domain_hash(&title_domain_hash)
                    .await
                    .unwrap_or(false)
                {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Article with title_domain_hash {} already processed (second check), skipping.",
                        title_domain_hash
                    );
                    return;
                }

                let mut llm_params = LLMParams {
                    llm_client,
                    model,
                    temperature,
                };

                let (
                    summary,
                    tiny_summary,
                    critical_analysis,
                    logical_fallacies,
                    source_analysis,
                    _relation,
                ) = process_analysis(
                    &article_text,
                    &article_html,
                    &article_url,
                    None,
                    &mut llm_params,
                )
                .await;

                let mut relation_to_topic = String::new();

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
                    let affected_places_str = affected_places
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ");
                    let how_prompt =
                        prompts::how_does_it_affect_prompt(&article_text, &affected_places_str);
                    let how_response = generate_llm_response(&how_prompt, &llm_params)
                        .await
                        .unwrap_or_default();
                    relation_to_topic
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
                    let why_not_response = generate_llm_response(&why_not_prompt, &llm_params)
                        .await
                        .unwrap_or_default();
                    relation_to_topic.push_str(&format!(
                        "\n\n{}\n\n{}",
                        non_affected_summary, why_not_response
                    ));
                }

                if !summary.is_empty()
                    || !critical_analysis.is_empty()
                    || !logical_fallacies.is_empty()
                    || !relation_to_topic.is_empty()
                    || !source_analysis.is_empty()
                {
                    let detailed_response_json = json!({
                        "topic": format!("{} {}", affected_summary, non_affected_summary),
                        "summary": summary,
                        "tiny_summary": tiny_summary,
                        "critical_analysis": critical_analysis,
                        "logical_fallacies": logical_fallacies,
                        "relation_to_topic": relation_to_topic,
                        "source_analysis": source_analysis,
                        "elapsed_time": start_time.elapsed().as_secs_f64(),
                        "model": model
                    });

                    // Check again if the article hash already exists in the database before posting to Slack
                    if db.has_hash(&article_hash).await.unwrap_or(false) {
                        info!(
                            target: TARGET_LLM_REQUEST,
                            "Article with hash {} was already processed (third check), skipping Slack post.",
                            article_hash
                        );
                        return;
                    }
                    // Check again if the title_domain_hash already exists in the database before posting to Slack
                    if db
                        .has_title_domain_hash(&title_domain_hash)
                        .await
                        .unwrap_or(false)
                    {
                        info!(
                            target: TARGET_LLM_REQUEST,
                            "Article with title_domain_hash {} already processed (third check), skipping.",
                            title_domain_hash
                        );
                        return;
                    }

                    send_to_slack(
                        &format!("*<{}|{}>*", article_url, article_title),
                        &detailed_response_json.to_string(),
                        slack_token,
                        default_slack_channel,
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
                        error!(target: TARGET_LLM_REQUEST, "Failed to update database: {:?}", e);
                    }
                }
                // Currently just logging the pulled data. Further processing can be added here.
            }
            Ok(None) => {
                debug!(
                    target: TARGET_LLM_REQUEST,
                    "worker {}: No items in life safety queue. Moving to matched topics queue.",
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
                        let mut llm_params = LLMParams {
                            llm_client,
                            model,
                            temperature,
                        };

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
                            &mut llm_params,
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
                            "model": model
                        });

                        send_to_slack(
                            &format!("*<{}|{}>*", article_url, article_title),
                            &response_json.to_string(),
                            slack_token,
                            default_slack_channel,
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
                        sleep(Duration::from_secs(10)).await;
                    }
                    Err(e) => {
                        error!(
                            target: TARGET_LLM_REQUEST,
                            "worker {}: Error fetching from matched topics queue: {:?}", worker_id, e
                        );
                        sleep(Duration::from_secs(5)).await;
                    }
                }
            }
            Err(e) => {
                error!(
                    target: TARGET_LLM_REQUEST,
                    "worker {}: Error fetching from life safety queue: {:?}", worker_id, e
                );
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn process_analysis(
    article_text: &str,
    article_html: &str,
    article_url: &str,
    topic: Option<&str>,
    llm_params: &mut LLMParams<'_>,
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
