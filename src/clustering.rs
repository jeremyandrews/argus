use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{self, Row};
use std::collections::HashMap;

use crate::db::cluster;
use crate::db::core::Database;
use crate::entity::types::EntityType;
use crate::llm::generate_llm_response;
use crate::{LLMClient, LLMParams, WorkerDetail};

/// Minimum similarity score required to assign an article to an existing cluster
pub const MIN_CLUSTER_SIMILARITY: f64 = 0.60;

/// Maximum number of articles to consider when generating a cluster summary
const MAX_SUMMARY_ARTICLES: usize = 10;

/// Struct representing an article cluster
#[derive(Debug, Serialize, Deserialize)]
pub struct ArticleCluster {
    pub id: i64,
    pub creation_date: String,
    pub last_updated: String,
    pub primary_entity_ids: Vec<i64>,
    pub summary: Option<String>,
    pub summary_version: i32,
    pub article_count: i32,
    pub importance_score: f64,
    pub has_timeline: bool,
    pub needs_summary_update: bool,
}

/// Struct representing a cluster timeline event
#[derive(Debug, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub event_date: String,
    pub headline: String,
    pub description: String,
    pub article_id: i64,
    pub importance: i32,
}

/// Assigns an article to the most appropriate cluster based on entity overlap
///
/// This function:
/// 1. Retrieves the article's extracted entities
/// 2. Finds the best matching cluster based on entity overlap
/// 3. Creates a new cluster if no good match is found
/// 4. Updates the article with the assigned cluster_id
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article to assign to a cluster
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the cluster the article was assigned to (0 if skipped)
/// * `Err` - If there was an error during the process
pub async fn assign_article_to_cluster(db: &Database, article_id: i64) -> Result<i64> {
    cluster::assign_article_to_cluster(db, article_id).await
}

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
    let articles = get_cluster_articles(db, cluster_id, MAX_SUMMARY_ARTICLES).await?;

    if articles.is_empty() {
        return Err(anyhow!("No articles found for cluster {}", cluster_id));
    }

    // Get entities for the cluster
    let entity_details = get_cluster_entity_details(db, cluster_id).await?;

    // Create a prompt for the LLM to generate a summary
    let prompt = build_summary_prompt(&articles, &entity_details)?;

    // Create LLM parameters
    let llm_params = LLMParams {
        llm_client: llm_client.clone(),
        model: "".to_string(), // Will be set by the LLM client
        temperature: 0.2,      // Lower temperature for more consistent summaries
        require_json: None,
        json_format: None,
    };

    // Generate the summary
    let summary = match generate_llm_response(&prompt, &llm_params, &worker_detail).await {
        Some(response) => response,
        None => return Err(anyhow!("Failed to generate summary")),
    };

    // Update the cluster with the new summary
    update_cluster_summary(db, cluster_id, &summary).await?;

    Ok(summary)
}

/// Gets articles in a cluster, ordered by recency and importance
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
/// * `limit` - Maximum number of articles to retrieve
///
/// # Returns
/// * `Ok(Vec<ClusterArticle>)` - Vector of articles in the cluster
/// * `Err` - If there was an error during retrieval
async fn get_cluster_articles(
    db: &Database,
    cluster_id: i64,
    limit: usize,
) -> Result<Vec<ClusterArticle>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.title, a.url, a.json_data, a.pub_date, a.tiny_summary, acm.similarity_score
        FROM articles a
        JOIN article_cluster_mappings acm ON a.id = acm.article_id
        WHERE acm.cluster_id = ?
        ORDER BY a.pub_date DESC, acm.similarity_score DESC
        LIMIT ?
        "#,
    )
    .bind(cluster_id)
    .bind(limit as i64)
    .fetch_all(db.pool())
    .await?;

    let mut articles = Vec::new();

    for row in rows {
        let article = ClusterArticle {
            id: row.get("id"),
            title: row.get("title"),
            url: row.get("url"),
            json_data: row.get("json_data"),
            pub_date: row.get("pub_date"),
            tiny_summary: row.get("tiny_summary"),
            similarity_score: row.get("similarity_score"),
        };

        articles.push(article);
    }

    Ok(articles)
}

/// Gets entity details for a cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
///
/// # Returns
/// * `Ok(HashMap<i64, EntityDetail>)` - Map of entity IDs to details
/// * `Err` - If there was an error during retrieval
async fn get_cluster_entity_details(
    db: &Database,
    cluster_id: i64,
) -> Result<HashMap<i64, EntityDetail>> {
    // First get the primary entity IDs for this cluster
    let cluster = sqlx::query(
        r#"
        SELECT primary_entity_ids FROM article_clusters
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_one(db.pool())
    .await?;

    let primary_entity_ids: String = cluster.get("primary_entity_ids");
    let entity_ids: Vec<i64> = serde_json::from_str(&primary_entity_ids)?;

    if entity_ids.is_empty() {
        return Ok(HashMap::new());
    }

    // Now get details for each entity
    let mut entity_details = HashMap::new();

    for entity_id in entity_ids {
        let row = sqlx::query(
            r#"
            SELECT e.id, e.canonical_name, e.entity_type
            FROM entities e
            WHERE e.id = ?
            "#,
        )
        .bind(entity_id)
        .fetch_optional(db.pool())
        .await?;

        if let Some(row) = row {
            let entity_type_str: String = row.get("entity_type");
            let entity_type = match entity_type_str.as_str() {
                "PERSON" => EntityType::Person,
                "ORGANIZATION" => EntityType::Organization,
                "LOCATION" => EntityType::Location,
                "EVENT" => EntityType::Event,
                "PRODUCT" => EntityType::Product,
                "DATE" => EntityType::Date,
                _ => EntityType::Other,
            };

            let detail = EntityDetail {
                id: row.get("id"),
                name: row.get("canonical_name"),
                entity_type,
            };

            entity_details.insert(entity_id, detail);
        }
    }

    Ok(entity_details)
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
            EntityType::Person => key_people.push(detail.name.clone()),
            EntityType::Organization => key_organizations.push(detail.name.clone()),
            EntityType::Location => key_locations.push(detail.name.clone()),
            EntityType::Event => key_events.push(detail.name.clone()),
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

/// Updates a cluster's summary in the database
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
/// * `summary` - The new summary
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
async fn update_cluster_summary(db: &Database, cluster_id: i64, summary: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET summary = ?,
            summary_version = summary_version + 1,
            needs_summary_update = 0
        WHERE id = ?
        "#,
    )
    .bind(summary)
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Calculates the importance score for a cluster
///
/// The score is based on:
/// - Number of articles (more articles = higher score)
/// - Average quality of articles (higher quality = higher score)
/// - Recency of updates (more recent = higher score)
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
///
/// # Returns
/// * `Ok(f64)` - The calculated importance score
/// * `Err` - If there was an error during calculation
pub async fn calculate_cluster_significance(db: &Database, cluster_id: i64) -> Result<f64> {
    // Get the cluster's article count and last updated date
    let cluster = sqlx::query(
        r#"
        SELECT article_count, last_updated FROM article_clusters
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_one(db.pool())
    .await?;

    let article_count: i32 = cluster.get("article_count");
    let last_updated: String = cluster.get("last_updated");

    // Get the average quality of articles in this cluster
    let avg_quality = sqlx::query(
        r#"
        SELECT AVG(a.quality) as avg_quality
        FROM articles a
        JOIN article_cluster_mappings acm ON a.id = acm.article_id
        WHERE acm.cluster_id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_one(db.pool())
    .await?;

    let avg_quality: f64 = avg_quality
        .get::<Option<f64>, _>("avg_quality")
        .unwrap_or(0.0);

    // Calculate recency factor (1.0 for very recent, decreasing over time)
    let now = Utc::now();
    let last_updated_date = chrono::DateTime::parse_from_rfc3339(&last_updated)
        .map_err(|_| anyhow!("Invalid last_updated date format"))?;

    let days_since_update = (now.timestamp() - last_updated_date.timestamp()) as f64 / 86400.0;
    let recency_factor = 1.0 / (1.0 + days_since_update / 7.0); // Halve importance after 7 days

    // Calculate the final score:
    // - Base score from article count (logarithmic scaling)
    // - Quality multiplier from -0.5 to 1.5
    // - Recency multiplier from 0.0 to 1.0
    let article_factor = (1.0 + article_count as f64).ln();
    let quality_multiplier = 1.0 + (avg_quality / 4.0); // -2 to +2 becomes -0.5 to +0.5

    let score = article_factor * quality_multiplier * recency_factor;

    // Update the cluster with the new importance score
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET importance_score = ?
        WHERE id = ?
        "#,
    )
    .bind(score)
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    Ok(score)
}

/// Struct representing an article in a cluster
#[allow(dead_code)]
pub struct ClusterArticle {
    pub id: i64,
    pub title: Option<String>,
    pub url: String,
    pub json_data: Option<String>,
    pub pub_date: Option<String>,
    pub tiny_summary: Option<String>,
    pub similarity_score: f64,
}

/// Struct representing entity details
#[allow(dead_code)]
pub struct EntityDetail {
    pub id: i64,
    pub name: String,
    pub entity_type: EntityType,
}
