use crate::llm::generate_llm_response;
use crate::prompt;
use crate::{LLMParams, WorkerDetail};
use tracing::{debug, warn};

/// Function to perform the analysis on an article.
/// Returns a tuple containing various analysis results.
pub async fn process_analysis(
    article_text: &str,
    article_html: &str,
    article_url: &str,
    topic: Option<&str>,
    pub_date: Option<&str>,
    llm_params: &mut LLMParams,
    worker_detail: &WorkerDetail,
) -> (
    String,         // summary
    String,         // tiny_summary
    String,         // tiny_title
    String,         // critical_analysis
    String,         // logical_fallacies
    String,         // source_analysis
    Option<String>, // relation
    u8,             // sources_quality
    u8,             // argument_quality
    String,         // source_type
    String,         // additional_insights
    String,         // action_recommendations
    String,         // talking_points
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
            String::new(),
            String::new(),
        );
    }

    // Start with summary to establish base understanding
    let summary_prompt = prompt::summary_prompt(article_text, pub_date);
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
                String::new(),
                String::new(),
            );
        }
    };

    // Only proceed with other analyses if we have a valid summary
    let tiny_summary = generate_llm_response(
        &prompt::tiny_summary_prompt(&summary),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    let tiny_title = generate_llm_response(
        &prompt::tiny_title_prompt(&tiny_summary, &summary),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Critical analysis and logical fallacies need the full article text
    let critical_analysis = generate_llm_response(
        &prompt::critical_analysis_prompt(article_text, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    let logical_fallacies = generate_llm_response(
        &prompt::logical_fallacies_prompt(article_text, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Source analysis needs HTML and URL
    let source_analysis = generate_llm_response(
        &prompt::source_analysis_prompt(article_html, article_url, pub_date),
        llm_params,
        worker_detail,
    )
    .await
    .unwrap_or_default();

    // Quality scores should only be generated if we have valid analyses
    let sources_quality = if !critical_analysis.is_empty() {
        generate_llm_response(
            &prompt::sources_quality_prompt(&critical_analysis),
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
            &prompt::argument_quality_prompt(&logical_fallacies),
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
            &prompt::source_type_prompt(&source_analysis, article_url),
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
            &prompt::relation_to_topic_prompt(article_text, topic, pub_date),
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
            &prompt::additional_insights_prompt(article_text, pub_date),
            llm_params,
            worker_detail,
        )
        .await
        .unwrap_or_default()
    } else {
        String::new()
    };

    // Generate action recommendations
    let action_recommendations = if !summary.is_empty() {
        generate_llm_response(
            &prompt::action_recommendations_prompt(article_text, pub_date),
            llm_params,
            worker_detail,
        )
        .await
        .unwrap_or_default()
    } else {
        String::new()
    };

    // Generate talking points
    let talking_points = if !summary.is_empty() {
        generate_llm_response(
            &prompt::talking_points_prompt(article_text, pub_date),
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
        action_recommendations,
        talking_points,
    )
}
