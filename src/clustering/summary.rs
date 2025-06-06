use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tracing::info;

use crate::clustering::types::{ClusterArticle, EntityDetail};
use crate::db::cluster;
use crate::db::core::Database;
use crate::llm::generate_text_response;
use crate::{LLMClient, LLMParamsBase, TextLLMParams, WorkerDetail};

/// Gets a list of clusters that need summary updates
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(Vec<i64>)` - Vector of cluster IDs that need summary updates
/// * `Err` - If there was an error during retrieval
pub async fn get_clusters_needing_summary_updates(db: &Database) -> Result<Vec<i64>> {
    cluster::get_clusters_needing_summary_updates(db).await
}

/// Generates a summary for a cluster based on its articles
///
/// # Arguments
/// * `db` - Database instance
/// * `llm_client` - LLM client to use for summary generation
/// * `cluster_id` - ID of the cluster to summarize
/// * `model_name` - Name of the model to use for generation
///
/// # Returns
/// * `Ok(String)` - The generated summary
/// * `Err` - If there was an error during summary generation
pub async fn generate_cluster_summary(
    db: &Database,
    llm_client: &LLMClient,
    cluster_id: i64,
    model_name: &str,
) -> Result<String> {
    // Create a worker detail for logging
    let worker_detail = WorkerDetail {
        name: "cluster summarizer".to_string(),
        id: 0,
        model: "summary model".to_string(),
        connection_info: "cluster_summary".to_string(),
    };

    // Get articles in this cluster
    let articles =
        cluster::get_cluster_articles(db, cluster_id, crate::clustering::MAX_SUMMARY_ARTICLES)
            .await?;

    if articles.is_empty() {
        return Err(anyhow!("No articles found for cluster {}", cluster_id));
    }

    // Get entities for the cluster
    let entity_details = cluster::get_cluster_entity_details(db, cluster_id).await?;

    // Create a prompt for the LLM to generate a summary
    let prompt = build_summary_prompt(&articles, &entity_details)?;

    // Create LLM parameters
    let llm_params = TextLLMParams {
        base: LLMParamsBase {
            llm_client: llm_client.clone(),
            model: model_name.to_string(),
            temperature: 0.2,   // Lower temperature for more consistent summaries
            model_config: None, // No model config needed for cluster summaries
            no_think: false,    // No need for special no_think mode for summaries
            context_window: Some(16384), // 2x context window for cluster summaries
        },
    };

    info!(
        "Generating cluster summary for cluster {} using model: {}",
        cluster_id, model_name
    );

    // Generate the summary
    let summary = match generate_text_response(&prompt, &llm_params, &worker_detail).await {
        Some(response) => response.to_string(),
        None => return Err(anyhow!("Failed to generate summary")),
    };

    // Update the cluster with the new summary
    cluster::update_cluster_summary(db, cluster_id, &summary).await?;

    Ok(summary)
}

/// Convert database quality score (-2 to 4) to readable label based on scoring.rs scale
fn quality_score_to_label(score: i8) -> &'static str {
    match score {
        4 | 3 => "EXCELLENT QUALITY",  // 3 = Excellent (green) in scoring.rs
        2 | 1 => "MODERATE QUALITY",   // 2 = Moderate (yellow) in scoring.rs
        0 | -1 | -2 => "POOR QUALITY", // 1 = Poor (red) in scoring.rs
        _ => "UNKNOWN QUALITY",
    }
}

/// Builds a prompt for generating a cluster summary
///
/// # Arguments
/// * `articles` - Articles in the cluster
/// * `entity_details` - Details of entities in the cluster
///
/// # Returns
/// * `Ok(String)` - The generated prompt
/// * `Err` - If there was an error during prompt building
fn build_summary_prompt(
    articles: &[ClusterArticle],
    entity_details: &HashMap<i64, EntityDetail>,
) -> Result<String> {
    let mut article_summaries = String::new();
    let mut high_quality_articles = Vec::new();
    let mut medium_quality_articles = Vec::new();
    let mut low_quality_articles = Vec::new();

    // Categorize articles by quality and build detailed summaries
    for (i, article) in articles.iter().enumerate() {
        let quality_label = quality_score_to_label(article.quality_score);

        let article_entry = format!(
            "Article {}: [{}] {} (Quality: {})\nTitle: {}\nSummary: {}\nURL: {}\n\n",
            i + 1,
            article.pub_date.as_deref().unwrap_or("Unknown date"),
            quality_label,
            article.quality_score,
            article.title.as_deref().unwrap_or("Untitled"),
            article.tiny_summary.as_deref().unwrap_or(""),
            article.url
        );

        article_summaries.push_str(&article_entry);

        // Categorize for reference section
        match article.quality_score {
            3 => high_quality_articles.push((i + 1, article)),
            2 => medium_quality_articles.push((i + 1, article)),
            1 => low_quality_articles.push((i + 1, article)),
            _ => low_quality_articles.push((i + 1, article)),
        }
    }

    // Extract key entities
    let mut key_people = Vec::new();
    let mut key_organizations = Vec::new();
    let mut key_locations = Vec::new();
    let mut key_events = Vec::new();

    for detail in entity_details.values() {
        match detail.entity_type {
            crate::entity::types::EntityType::Person => key_people.push(detail.name.clone()),
            crate::entity::types::EntityType::Organization => {
                key_organizations.push(detail.name.clone())
            }
            crate::entity::types::EntityType::Location => key_locations.push(detail.name.clone()),
            crate::entity::types::EntityType::Event => key_events.push(detail.name.clone()),
            _ => {}
        }
    }

    // Build quality context
    let quality_guidance = if !low_quality_articles.is_empty() {
        format!(
            "\n\nIMPORTANT QUALITY CONSIDERATIONS:\n- {} HIGH quality articles (most reliable)\n- {} MEDIUM quality articles (generally trustworthy)\n- {} LOW quality articles (use with caution)\n\nPrioritize information from high-quality sources. When including information from low-quality sources, clearly indicate the source reliability concerns.",
            high_quality_articles.len(),
            medium_quality_articles.len(),
            low_quality_articles.len()
        )
    } else {
        String::new()
    };

    // Build the prompt
    let prompt = format!(
        r#"You are creating an executive summary of related news articles for a busy executive who will read this INSTEAD of the individual articles. This summary serves as their primary source of information on this topic.

KEY ENTITIES MENTIONED ACROSS ARTICLES:
People: {}
Organizations: {}
Locations: {}
Events: {}{}

ARTICLE SUMMARIES:
{}

Create a comprehensive summary following this EXACT structure:

**TL;DR:** [Write 1-2 sentences capturing the essential story - what happened, who was involved, and why it matters]

**Full Summary:**
[Write a detailed narrative that:]
- Prioritizes information from HIGH quality sources over medium/low quality sources
- Presents information chronologically when relevant
- Uses phrases like "according to [Article X]" or "reported by reliable sources" for attribution
- For any claims from LOW quality sources, use qualifying language like "according to unverified reports" or "sources suggest"
- Scales length based on story complexity (simple stories: 200-400 words, complex stories: 400-800 words)
- Maintains neutral, professional tone suitable for executive briefing
- Ensures all critical entities and developments are covered

**Quality Notes:**
[If any information comes from low-quality sources, briefly note: "Some details in this summary come from sources with reliability concerns: [specific claims and source references]"]

**References:**
[List all source articles in quality order:]
High Quality Sources:
[List high-quality articles with titles and dates]

Medium Quality Sources:
[List medium-quality articles with titles and dates]

Low Quality Sources:
[List low-quality articles with titles and dates, if any]

Remember: This summary replaces reading individual articles, so ensure completeness while maintaining appropriate skepticism about lower-quality sources."#,
        key_people.join(", "),
        key_organizations.join(", "),
        key_locations.join(", "),
        key_events.join(", "),
        quality_guidance,
        article_summaries
    );

    Ok(prompt)
}
