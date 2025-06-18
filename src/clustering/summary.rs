use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tracing::{error, info};

use crate::clustering::types::{ClusterArticle, CurrentArticleData, EntityDetail};
use crate::db::cluster;
use crate::db::core::Database;
use crate::llm::generate_text_response;
use crate::{TextLLMParams, WorkerDetail};

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
/// * `llm_params` - LLM parameters to use for summary generation
/// * `cluster_id` - ID of the cluster to summarize
/// * `current_article` - Optional current article data (for articles being processed)
///
/// # Returns
/// * `Ok(String)` - The generated summary
/// * `Err` - If there was an error during summary generation
pub async fn generate_cluster_summary(
    db: &Database,
    llm_params: &TextLLMParams,
    cluster_id: i64,
    current_article: Option<CurrentArticleData>,
) -> Result<String> {
    // Create a worker detail for logging
    let worker_detail = WorkerDetail {
        name: "cluster summarizer".to_string(),
        id: 0,
        model: llm_params.base.model.clone(),
        connection_info: "cluster_summary".to_string(),
    };

    // Get articles in this cluster
    let articles =
        cluster::get_cluster_articles(db, cluster_id, crate::clustering::MAX_SUMMARY_ARTICLES)
            .await?;

    if articles.is_empty() && current_article.is_none() {
        return Err(anyhow!("No articles found for cluster {}", cluster_id));
    }

    // Get entities for the cluster
    let entity_details = cluster::get_cluster_entity_details(db, cluster_id).await?;

    // Create a prompt for the LLM to generate a summary
    let prompt = build_summary_prompt(&articles, &entity_details, current_article.as_ref())?;

    info!(
        "Generating cluster summary for cluster {} using model: {}",
        cluster_id, llm_params.base.model
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

/// Formats article title with tiny_title and original title
/// Format: "Tiny Title (Original Title)" or just the available title if only one exists
fn format_article_title(tiny_title: Option<&str>, original_title: Option<&str>) -> String {
    match (tiny_title, original_title) {
        (Some(tiny), Some(orig)) if tiny == orig => tiny.to_string(),
        (Some(tiny), Some(orig)) => format!("{} ({})", tiny, orig),
        (Some(tiny), None) => tiny.to_string(),
        (None, Some(orig)) => orig.to_string(),
        (None, None) => "Untitled".to_string(),
    }
}

/// Builds a prompt for generating a cluster summary
///
/// # Arguments
/// * `articles` - Articles in the cluster
/// * `entity_details` - Details of entities in the cluster
/// * `current_article` - Optional current article data (for articles being processed)
///
/// # Returns
/// * `Ok(String)` - The generated prompt
/// * `Err` - If there was an error during prompt building
fn build_summary_prompt(
    articles: &[ClusterArticle],
    entity_details: &HashMap<i64, EntityDetail>,
    current_article: Option<&CurrentArticleData>,
) -> Result<String> {
    let mut analysis_section = String::new();
    let mut reference_data = String::new();
    let mut excellent_articles = Vec::new();
    let mut moderate_articles = Vec::new();
    let mut low_articles = Vec::new();
    let mut article_counter = 0;

    // Add current article first if provided
    if let Some(current) = current_article {
        article_counter += 1;
        let quality_tier = map_quality_to_tier(current.quality_score);
        let quality_label = quality_tier_to_label(quality_tier);
        let source_name = extract_source_name(&current.url);
        let formatted_title = format_article_title(Some(&current.tiny_title), Some(&current.title));

        // Build analysis section: Brief content for AI analysis
        analysis_section.push_str(&format!(
            "Article {} ({}): {}\n",
            article_counter, quality_label, current.tiny_summary
        ));

        // Build reference data: Complete metadata
        reference_data.push_str(&format!(
            "Article {}: [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: {}\n\n",
            article_counter,
            current.pub_date.as_deref().unwrap_or("Unknown date"),
            formatted_title,
            source_name,
            current.url,
            current.tiny_summary,
            quality_tier
        ));

        // Categorize by quality tier
        match quality_tier {
            3 => excellent_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                current
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                current.url.clone(),
                current.tiny_summary.clone(),
            )),
            2 => moderate_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                current
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                current.url.clone(),
                current.tiny_summary.clone(),
            )),
            1 => low_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                current
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                current.url.clone(),
                current.tiny_summary.clone(),
            )),
            _ => low_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                current
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                current.url.clone(),
                current.tiny_summary.clone(),
            )),
        }
    }

    // Process existing cluster articles and map quality scores to standardized tiers
    for article in articles.iter() {
        article_counter += 1;
        let quality_tier = map_quality_to_tier(article.quality_score);
        let quality_label = quality_tier_to_label(quality_tier);
        let source_name = extract_source_name(&article.url);

        // Use format_article_title to get properly formatted title
        let formatted_title =
            format_article_title(article.tiny_summary.as_deref(), article.title.as_deref());

        // Build analysis section: Brief content for AI analysis
        let brief_content = article
            .tiny_summary
            .as_deref()
            .or(article.title.as_deref())
            .unwrap_or("No summary available");

        analysis_section.push_str(&format!(
            "Article {} ({}): {}\n",
            article_counter, quality_label, brief_content
        ));

        // Build reference data: Complete metadata
        reference_data.push_str(&format!(
            "Article {}: [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: {}\n\n",
            article_counter,
            article.pub_date.as_deref().unwrap_or("Unknown date"),
            formatted_title,
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
            3 => excellent_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                article
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                article.url.clone(),
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
                    .to_string(),
            )),
            2 => moderate_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                article
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                article.url.clone(),
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
                    .to_string(),
            )),
            1 => low_articles.push((
                article_counter,
                formatted_title.clone(),
                source_name.clone(),
                article
                    .pub_date
                    .as_deref()
                    .unwrap_or("Unknown date")
                    .to_string(),
                article.url.clone(),
                article
                    .tiny_summary
                    .as_deref()
                    .unwrap_or("No summary available")
                    .to_string(),
            )),
            _ => {
                error!(
                    "Invalid quality tier {} for article {}",
                    quality_tier, article.id
                );
                low_articles.push((
                    article_counter,
                    formatted_title.clone(),
                    source_name.clone(),
                    article
                        .pub_date
                        .as_deref()
                        .unwrap_or("Unknown date")
                        .to_string(),
                    article.url.clone(),
                    article
                        .tiny_summary
                        .as_deref()
                        .unwrap_or("No summary available")
                        .to_string(),
                ));
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

    // Build references section with proper title formatting
    let mut references_section = String::new();

    if !excellent_articles.is_empty() {
        references_section.push_str("Excellent Quality Sources:\n");
        for (num, title, source_name, pub_date, url, summary) in &excellent_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 3\n\n",
                num, pub_date, title, source_name, url, summary
            ));
        }
        references_section.push('\n');
    }

    if !moderate_articles.is_empty() {
        references_section.push_str("Moderate Quality Sources:\n");
        for (num, title, source_name, pub_date, url, summary) in &moderate_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 2\n\n",
                num, pub_date, title, source_name, url, summary
            ));
        }
        references_section.push('\n');
    }

    if !low_articles.is_empty() {
        references_section.push_str("Low Quality Sources:\n");
        for (num, title, source_name, pub_date, url, summary) in &low_articles {
            references_section.push_str(&format!(
                "{}. [{}] \"{}\" - {}\n   URL: {}\n   Summary: {}\n   Quality: 1\n\n",
                num, pub_date, title, source_name, url, summary
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
