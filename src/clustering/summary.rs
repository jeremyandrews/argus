use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tracing::{error, info};

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

/// Maps raw quality scores to standardized 3-tier system
pub fn map_quality_to_tier(raw_score: i8) -> i8 {
    match raw_score {
        4 | 3 => 3,       // Excellent → Tier 3
        2 | 1 => 2,       // Moderate → Tier 2
        0 | -1 | -2 => 1, // Low → Tier 1
        _ => {
            error!("Unknown quality score encountered: {}", raw_score);
            1 // Default to Low and continue
        }
    }
}

/// Maps quality tier to human-readable label
pub fn quality_tier_to_label(tier: i8) -> &'static str {
    match tier {
        3 => "Excellent",
        2 => "Moderate",
        1 => "Low",
        _ => {
            error!("Invalid quality tier: {}", tier);
            "Unknown"
        }
    }
}

/// Extracts readable source name from URL
pub fn extract_source_name(url: &str) -> String {
    let domain = url.split('/').nth(2).unwrap_or("Unknown Source");

    // Remove www. prefix and get base domain
    let clean_domain = domain
        .strip_prefix("www.")
        .unwrap_or(domain)
        .split('.')
        .next()
        .unwrap_or("Unknown");

    // Convert known domains to proper names
    match clean_domain {
        "burnabynow" => "Burnaby Now".to_string(),
        "denverbroncos" => "Denver Broncos".to_string(),
        "clickorlando" => "ClickOrlando".to_string(),
        "denver7" => "Denver7".to_string(),
        "9to5mac" => "9to5Mac".to_string(),
        "bitcoincore" => "Bitcoin Core".to_string(),
        "finance" => "Yahoo Finance".to_string(),
        "slashdot" => "Slashdot".to_string(),
        "lanazione" => "La Nazione".to_string(),
        "tag1consulting" => "Tag1 Consulting".to_string(),
        "tomshardware" => "Tom's Hardware".to_string(),
        "ycombinator" => "Hacker News".to_string(),
        _ => title_case(clean_domain),
    }
}

/// Converts a string to title case
fn title_case(s: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    s.chars()
        .next()
        .unwrap_or_default()
        .to_uppercase()
        .collect::<String>()
        + &s[1..].to_lowercase()
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
    let mut analysis_section = String::new();
    let mut reference_data = String::new();
    let mut excellent_articles = Vec::new();
    let mut moderate_articles = Vec::new();
    let mut low_articles = Vec::new();

    // Process articles and map quality scores to standardized tiers
    for (i, article) in articles.iter().enumerate() {
        let quality_tier = map_quality_to_tier(article.quality_score);
        let quality_label = quality_tier_to_label(quality_tier);
        let source_name = extract_source_name(&article.url);

        // Build analysis section: Brief content for AI analysis
        let brief_content = article
            .tiny_summary
            .as_deref()
            .or(article.title.as_deref())
            .unwrap_or("No summary available");

        analysis_section.push_str(&format!(
            "Article {} ({}): {}\n",
            i + 1,
            quality_label,
            brief_content
        ));

        // Build reference data: Complete metadata
        reference_data.push_str(&format!(
            "Article {}: [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: {}\n\n",
            i + 1,
            article.pub_date.as_deref().unwrap_or("Unknown date"),
            article.title.as_deref().unwrap_or("Untitled"),
            source_name,
            article.url,
            article
                .tiny_summary
                .as_deref()
                .unwrap_or("No summary available"),
            quality_tier
        ));

        // Categorize by standardized quality tiers
        match quality_tier {
            3 => excellent_articles.push((i + 1, article, source_name)),
            2 => moderate_articles.push((i + 1, article, source_name)),
            1 => low_articles.push((i + 1, article, source_name)),
            _ => {
                error!(
                    "Invalid quality tier {} for article {}",
                    quality_tier, article.id
                );
                low_articles.push((i + 1, article, source_name));
            }
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

    // Build quality distribution context
    let quality_guidance = format!(
        "\n\nQUALITY DISTRIBUTION:\n- {} Excellent quality articles (most reliable)\n- {} Moderate quality articles (generally trustworthy)\n- {} Low quality articles (use with caution)\n\nPrioritize information from Excellent sources. When using information from Low quality sources, use qualifying language.",
        excellent_articles.len(),
        moderate_articles.len(),
        low_articles.len()
    );

    // Build references section
    let mut references_section = String::new();

    if !excellent_articles.is_empty() {
        references_section.push_str("Excellent Quality Sources:\n");
        for (num, article, source_name) in &excellent_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 3\n\n",
                num,
                article.pub_date.as_deref().unwrap_or("Unknown date"),
                article.title.as_deref().unwrap_or("Untitled"),
                source_name,
                article.url,
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
            ));
        }
        references_section.push('\n');
    }

    if !moderate_articles.is_empty() {
        references_section.push_str("Moderate Quality Sources:\n");
        for (num, article, source_name) in &moderate_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 2\n\n",
                num,
                article.pub_date.as_deref().unwrap_or("Unknown date"),
                article.title.as_deref().unwrap_or("Untitled"),
                source_name,
                article.url,
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
            ));
        }
        references_section.push('\n');
    }

    if !low_articles.is_empty() {
        references_section.push_str("Low Quality Sources:\n");
        for (num, article, source_name) in &low_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 1\n\n",
                num,
                article.pub_date.as_deref().unwrap_or("Unknown date"),
                article.title.as_deref().unwrap_or("Untitled"),
                source_name,
                article.url,
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
            ));
        }
    }

    // Build the prompt
    let prompt = format!(
        r#"You are creating an executive summary of related news articles for a busy executive who will read this INSTEAD of the individual articles. This summary serves as their primary source of information on this topic.

KEY ENTITIES MENTIONED ACROSS ARTICLES:
People: {}
Organizations: {}
Locations: {}
Events: {}{}

ANALYSIS SECTION (for content analysis):
{}

Create a comprehensive summary following this EXACT structure:

**TL;DR:** [Write 1-2 sentences capturing the essential story - what happened, who was involved, and why it matters]

**Full Summary:**
[Write a detailed narrative that:]
- Prioritizes information from Excellent quality sources over moderate/low quality sources
- Presents information chronologically when relevant
- Uses phrases like "according to [Article X]" or "reported by reliable sources" for attribution
- For any claims from Low quality sources, use qualifying language like "according to unverified reports" or "sources suggest"
- Scales length based on story complexity (simple stories: 200-400 words, complex stories: 400-800 words)
- Maintains neutral, professional tone suitable for executive briefing
- Ensures all critical entities and developments are covered

**Quality Notes:**
[If any information comes from low-quality sources, briefly note: "Some details in this summary come from sources with reliability concerns: [specific claims and source references]"]

**References:**
{}

COMPLETE ARTICLE METADATA (for reference generation):
{}

Remember: This summary replaces reading individual articles, so ensure completeness while maintaining appropriate skepticism about lower-quality sources."#,
        key_people.join(", "),
        key_organizations.join(", "),
        key_locations.join(", "),
        key_events.join(", "),
        quality_guidance,
        analysis_section,
        references_section,
        reference_data
    );

    Ok(prompt)
}
