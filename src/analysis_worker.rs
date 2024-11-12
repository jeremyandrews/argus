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
        match db.fetch_and_delete_from_matched_topics_queue().await {
            Ok(Some((article_text, article_html, article_url, article_title, topic))) => {
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
                    &topic,
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

                // Optionally, update the database with additional analysis details
                if let Err(e) = db
                    .add_article(
                        &article_url,
                        true,
                        Some(&topic),
                        Some(&response_json.to_string()),
                        Some(&tiny_summary),
                        None,
                        None,
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
}

async fn process_analysis(
    article_text: &str,
    article_html: &str,
    article_url: &str,
    topic: &str,
    llm_params: &mut LLMParams<'_>,
) -> (String, String, String, String, String, String) {
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
    let relation_prompt = prompts::relation_to_topic_prompt(article_text, topic);

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
    let relation_response = generate_llm_response(&relation_prompt, &llm_params)
        .await
        .unwrap_or_default();

    (
        summary,
        tiny_summary,
        critical_analysis,
        logical_fallacies,
        source_analysis,
        relation_response,
    )
}
