use anyhow::{anyhow, Result};
use std::collections::HashMap;

use crate::clustering::types::{ClusterArticle, EntityDetail};
use crate::db::cluster;
use crate::db::core::Database;
use crate::llm::generate_llm_response;
use crate::{LLMClient, LLMParams, WorkerDetail};

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
///
/// # Returns
/// * `Ok(String)` - The generated summary
/// * `Err` - If there was an error during summary generation
pub async fn generate_cluster_summary(
    db: &Database,
    llm_client: &LLMClient,
    cluster_id: i64,
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
    let llm_params = LLMParams {
        llm_client: llm_client.clone(),
        model: "".to_string(), // Will be set by the LLM client
        temperature: 0.2,      // Lower temperature for more consistent summaries
        require_json: None,
        json_format: None,
        thinking_config: None, // No thinking needed for cluster summaries
    };

    // Generate the summary
    let summary = match generate_llm_response(&prompt, &llm_params, &worker_detail).await {
        Some(response) => response,
        None => return Err(anyhow!("Failed to generate summary")),
    };

    // Update the cluster with the new summary
    cluster::update_cluster_summary(db, cluster_id, &summary).await?;

    Ok(summary)
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

    for (i, article) in articles.iter().enumerate() {
        article_summaries.push_str(&format!(
            "Article {}: [{}] {}\n{}\n\n",
            i + 1,
            article.pub_date.as_deref().unwrap_or("Unknown date"),
            article.title.as_deref().unwrap_or("Untitled"),
            article.tiny_summary.as_deref().unwrap_or("")
        ));
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

    // Build the prompt
    let prompt = format!(
        r#"You are tasked with creating a comprehensive summary of a collection of related news articles that all discuss the same topic or story.

KEY ENTITIES MENTIONED ACROSS ARTICLES:
People: {}
Organizations: {}
Locations: {}
Events: {}

ARTICLE SUMMARIES:
{}

Based on these article summaries and key entities, please write a comprehensive, well-structured summary that:
1. Captures the overall story or topic being discussed across all articles
2. Highlights the most important facts and developments
3. Presents information in chronological order where appropriate
4. Ensures all critical entities (people, organizations, locations, events) are included
5. Provides proper context to understand the significance of this story
6. Is written in a neutral, journalistic tone
7. Is approximately 250-400 words in length

Your summary should be cohesive and readable as a single piece, not just a collection of facts from individual articles. Focus on creating a narrative that helps the reader understand this topic thoroughly."#,
        key_people.join(", "),
        key_organizations.join(", "),
        key_locations.join(", "),
        key_events.join(", "),
        article_summaries
    );

    Ok(prompt)
}
