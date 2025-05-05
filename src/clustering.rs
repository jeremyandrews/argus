use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{self, Row};
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use tracing::{debug, info, warn};

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
        thinking_config: None, // No thinking needed for cluster summaries
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
/// Creates an empty cluster with default values
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the newly created cluster
/// * `Err` - If there was an error during creation
pub async fn create_empty_cluster(db: &Database) -> Result<i64> {
    let now = Utc::now().to_rfc3339();

    // Create the cluster with empty primary entities
    let cluster_id = sqlx::query(
        r#"
        INSERT INTO article_clusters
        (creation_date, last_updated, primary_entity_ids, article_count, needs_summary_update, status)
        VALUES (?, ?, '[]', 0, 1, 'active')
        "#,
    )
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await?
    .last_insert_rowid();

    debug!("Created new empty cluster {}", cluster_id);
    Ok(cluster_id)
}

/// Finds clusters with overlapping entities that are candidates for merging
///
/// # Arguments
/// * `db` - Database instance
/// * `min_overlap_ratio` - Minimum Jaccard similarity for considering clusters as candidates (0.0-1.0)
///
/// # Returns
/// * `Ok(Vec<Vec<i64>>)` - Groups of cluster IDs that are candidates for merging
/// * `Err` - If there was an error during processing
pub async fn find_clusters_with_entity_overlap(
    db: &Database,
    min_overlap_ratio: f64,
) -> Result<Vec<Vec<i64>>> {
    // Get all active clusters
    let active_clusters = sqlx::query(
        r#"
        SELECT id, primary_entity_ids 
        FROM article_clusters
        WHERE status = 'active'
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let mut cluster_groups: Vec<Vec<i64>> = Vec::new();
    let mut processed_clusters: HashSet<i64> = HashSet::new();

    // Compare each cluster with all others
    for i in 0..active_clusters.len() {
        let cluster_i_id: i64 = active_clusters[i].get("id");

        // Skip if this cluster is already in a merge group
        if processed_clusters.contains(&cluster_i_id) {
            continue;
        }

        let entities_i_json: String = active_clusters[i].get("primary_entity_ids");
        let entities_i: HashSet<i64> = serde_json::from_str::<Vec<i64>>(&entities_i_json)
            .map_err(|e| {
                anyhow!(
                    "Failed to parse entities for cluster {}: {}",
                    cluster_i_id,
                    e
                )
            })?
            .into_iter()
            .collect();

        // Skip if this cluster has no entities
        if entities_i.is_empty() {
            continue;
        }

        let mut merge_group = vec![cluster_i_id];

        for j in (i + 1)..active_clusters.len() {
            let cluster_j_id: i64 = active_clusters[j].get("id");

            // Skip if this cluster is already in a merge group
            if processed_clusters.contains(&cluster_j_id) {
                continue;
            }

            let entities_j_json: String = active_clusters[j].get("primary_entity_ids");
            let entities_j: HashSet<i64> = serde_json::from_str::<Vec<i64>>(&entities_j_json)
                .map_err(|e| {
                    anyhow!(
                        "Failed to parse entities for cluster {}: {}",
                        cluster_j_id,
                        e
                    )
                })?
                .into_iter()
                .collect();

            // Skip if this cluster has no entities
            if entities_j.is_empty() {
                continue;
            }

            // Calculate Jaccard similarity: |A ∩ B| / |A ∪ B|
            let intersection = entities_i.intersection(&entities_j).count();
            let union = entities_i.union(&entities_j).count();

            let similarity = intersection as f64 / union as f64;

            // If similarity exceeds threshold, add to merge group
            if similarity >= min_overlap_ratio {
                info!(
                    "Clusters {} and {} have entity overlap ratio {:.2}",
                    cluster_i_id, cluster_j_id, similarity
                );
                merge_group.push(cluster_j_id);
            }
        }

        // If we found multiple clusters to merge
        if merge_group.len() > 1 {
            // Mark all these clusters as processed
            for &cluster_id in &merge_group {
                processed_clusters.insert(cluster_id);
            }

            cluster_groups.push(merge_group);
        }
    }

    Ok(cluster_groups)
}

/// Combine all entity IDs from multiple clusters
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_ids` - IDs of clusters to combine entities from
///
/// # Returns
/// * `Ok(Vec<i64>)` - Combined unique entity IDs
/// * `Err` - If there was an error during processing
pub async fn combine_entities_from_clusters(
    db: &Database,
    cluster_ids: &[i64],
) -> Result<Vec<i64>> {
    if cluster_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Create a set to track unique entities
    let mut all_entities = HashSet::new();

    for &cluster_id in cluster_ids {
        // Get this cluster's entities
        let row = sqlx::query("SELECT primary_entity_ids FROM article_clusters WHERE id = ?")
            .bind(cluster_id)
            .fetch_one(db.pool())
            .await?;

        let entities_json: String = row.get("primary_entity_ids");
        let cluster_entities: Vec<i64> = serde_json::from_str(&entities_json)?;

        // Add all entities to the set
        all_entities.extend(cluster_entities);
    }

    // Convert set back to vector
    Ok(Vec::from_iter(all_entities))
}

/// Updates a cluster's primary entities
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to update
/// * `entity_ids` - New primary entity IDs
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
pub async fn update_cluster_primary_entities(
    db: &Database,
    cluster_id: i64,
    entity_ids: &[i64],
) -> Result<()> {
    let entity_ids_json = serde_json::to_string(entity_ids)?;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE article_clusters
        SET primary_entity_ids = ?,
            last_updated = ?
        WHERE id = ?
        "#,
    )
    .bind(&entity_ids_json)
    .bind(&now)
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Marks a cluster as merged into another cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to mark as merged
/// * `merged_into` - ID of the cluster it was merged into
/// * `reason` - Optional reason for the merge
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
pub async fn mark_cluster_as_merged(
    db: &Database,
    cluster_id: i64,
    merged_into: i64,
    reason: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    // Update cluster status
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET status = 'merged'
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    // Record merge history
    sqlx::query(
        r#"
        INSERT INTO cluster_merge_history
        (original_cluster_id, merged_into_cluster_id, merge_date, merge_reason)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(cluster_id)
    .bind(merged_into)
    .bind(&now)
    .bind(reason)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Transfers user preferences from source clusters to a target cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `source_cluster_ids` - IDs of source clusters
/// * `target_cluster_id` - ID of the target cluster
///
/// # Returns
/// * `Ok(())` - If the transfer was successful
/// * `Err` - If there was an error during the transfer
async fn transfer_user_preferences(
    db: &Database,
    source_cluster_ids: &[i64],
    target_cluster_id: i64,
) -> Result<()> {
    // Get all user IDs that have preferences for any source cluster
    let mut user_ids = HashSet::new();

    for &source_id in source_cluster_ids {
        let rows = sqlx::query("SELECT user_id FROM user_cluster_preferences WHERE cluster_id = ?")
            .bind(source_id)
            .fetch_all(db.pool())
            .await?;

        for row in rows {
            user_ids.insert(row.get::<i64, _>("user_id"));
        }
    }

    let now = Utc::now().to_rfc3339();

    // For each user, create a preference for the target cluster
    for user_id in user_ids {
        // Check if user already has preference for target
        let existing = sqlx::query(
            "SELECT 1 FROM user_cluster_preferences WHERE user_id = ? AND cluster_id = ?",
        )
        .bind(user_id)
        .bind(target_cluster_id)
        .fetch_optional(db.pool())
        .await?;

        if existing.is_some() {
            // User already has preference for target cluster, update it
            sqlx::query(
                r#"
                UPDATE user_cluster_preferences
                SET silenced = 0,  -- Ensure notifications are enabled
                    followed = 1,  -- Ensure followed is enabled
                    last_interaction = ?
                WHERE user_id = ? AND cluster_id = ?
                "#,
            )
            .bind(&now)
            .bind(user_id)
            .bind(target_cluster_id)
            .execute(db.pool())
            .await?;
        } else {
            // Create new preference
            sqlx::query(
                r#"
                INSERT INTO user_cluster_preferences
                (user_id, cluster_id, silenced, followed, last_interaction)
                VALUES (?, ?, 0, 1, ?)
                "#,
            )
            .bind(user_id)
            .bind(target_cluster_id)
            .bind(&now)
            .execute(db.pool())
            .await?;
        }
    }

    Ok(())
}

/// Merges multiple clusters into a new cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `source_cluster_ids` - IDs of clusters to merge
/// * `reason` - Optional reason for the merge
///
/// # Returns
/// * `Ok(i64)` - ID of the newly created merged cluster
/// * `Err` - If there was an error during the merge
pub async fn merge_clusters(
    db: &Database,
    source_cluster_ids: &[i64],
    reason: Option<&str>,
) -> Result<i64> {
    if source_cluster_ids.len() < 2 {
        return Err(anyhow!("At least two clusters are required for merging"));
    }

    // Step 1: Create new target cluster
    let new_cluster_id = create_empty_cluster(db).await?;
    info!(
        "Created new cluster {} for merge of {} clusters",
        new_cluster_id,
        source_cluster_ids.len()
    );

    // Step 2: Combine entities from all source clusters
    let combined_entities = combine_entities_from_clusters(db, source_cluster_ids).await?;
    update_cluster_primary_entities(db, new_cluster_id, &combined_entities).await?;

    let now = Utc::now().to_rfc3339();
    let mut total_articles = 0;

    // Step 3: Update all article mappings
    for &source_id in source_cluster_ids {
        // Get all articles from source cluster
        let articles = crate::db::cluster::get_cluster_articles(db, source_id, 10000).await?;
        total_articles += articles.len();

        // Create mappings to the new cluster
        for article in articles {
            // Add article to new cluster (preserve original similarity score)
            crate::db::cluster::assign_to_cluster(
                db,
                article.id,
                new_cluster_id,
                article.similarity_score,
            )
            .await?;

            // Update article's primary cluster_id reference
            crate::db::cluster::update_article_cluster_id(db, article.id, new_cluster_id).await?;
        }

        // Mark source cluster as merged
        mark_cluster_as_merged(db, source_id, new_cluster_id, reason).await?;
    }

    // Step 4: Update the article count for the new cluster
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET article_count = ?,
            last_updated = ?
        WHERE id = ?
        "#,
    )
    .bind(total_articles as i32)
    .bind(&now)
    .bind(new_cluster_id)
    .execute(db.pool())
    .await?;

    // Step 5: Update user preferences
    transfer_user_preferences(db, source_cluster_ids, new_cluster_id).await?;

    // Step 6: Generate a new summary
    let _summary = match generate_cluster_summary(
        db,
        &crate::vector::get_default_llm_client(),
        new_cluster_id,
    )
    .await
    {
        Ok(summary) => summary,
        Err(e) => {
            warn!(
                "Failed to generate summary for merged cluster {}: {}",
                new_cluster_id, e
            );
            String::from(
                "This is a merged cluster containing related articles from multiple topics.",
            )
        }
    };

    // Step 7: Calculate significance score for the new cluster
    let _score = match calculate_cluster_significance(db, new_cluster_id).await {
        Ok(score) => score,
        Err(e) => {
            warn!(
                "Failed to calculate significance for merged cluster {}: {}",
                new_cluster_id, e
            );
            0.0 // Return default value to fix type mismatch
        }
    };

    info!(
        "Successfully merged {} clusters into new cluster {} with {} articles",
        source_cluster_ids.len(),
        new_cluster_id,
        total_articles
    );

    Ok(new_cluster_id)
}

/// Get all clusters that have been merged and their destinations
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(Vec<(i64, i64)>)` - Vector of (original_id, merged_into_id) pairs
/// * `Err` - If there was an error during retrieval
pub async fn get_merged_clusters(db: &Database) -> Result<Vec<(i64, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT original_cluster_id, merged_into_cluster_id
        FROM cluster_merge_history
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let mut merged_pairs = Vec::new();
    for row in rows {
        let original_id: i64 = row.get("original_cluster_id");
        let merged_into_id: i64 = row.get("merged_into_cluster_id");

        merged_pairs.push((original_id, merged_into_id));
    }

    Ok(merged_pairs)
}

/// Checks if a cluster has been merged and returns the current active cluster ID
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to check
///
/// # Returns
/// * `Ok(Some(i64))` - ID of the cluster it was merged into, if applicable
/// * `Ok(None)` - If the cluster is still active
/// * `Err` - If there was an error during the check
pub async fn get_merged_cluster_destination(db: &Database, cluster_id: i64) -> Result<Option<i64>> {
    let row = sqlx::query(
        r#"
        SELECT merged_into_cluster_id 
        FROM cluster_merge_history
        WHERE original_cluster_id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row.map(|r| r.get::<i64, _>("merged_into_cluster_id")))
}

/// Gets the source clusters that were merged into this cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the merged cluster
///
/// # Returns
/// * `Ok(Vec<i64>)` - IDs of clusters that were merged into this one
/// * `Err` - If there was an error during retrieval
pub async fn get_clusters_merged_into(db: &Database, cluster_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"
        SELECT original_cluster_id
        FROM cluster_merge_history
        WHERE merged_into_cluster_id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_all(db.pool())
    .await?;

    let source_ids = rows
        .iter()
        .map(|row| row.get::<i64, _>("original_cluster_id"))
        .collect();

    Ok(source_ids)
}

/// Check if two clusters are temporally close (have articles published in similar timeframes)
async fn are_clusters_temporally_close(
    db: &Database,
    cluster_id1: i64,
    cluster_id2: i64,
    max_days_apart: i32,
) -> Result<bool> {
    // Get the most recent article date for each cluster
    let date1 = get_most_recent_article_date(db, cluster_id1).await?;
    let date2 = get_most_recent_article_date(db, cluster_id2).await?;

    match (date1, date2) {
        (Some(date1), Some(date2)) => {
            // Parse the dates
            let dt1 = chrono::DateTime::parse_from_rfc3339(&date1)?;
            let dt2 = chrono::DateTime::parse_from_rfc3339(&date2)?;

            // Calculate days difference
            let diff = (dt1.timestamp() - dt2.timestamp()).abs() as f64 / 86400.0;
            Ok(diff <= max_days_apart as f64)
        }
        _ => Ok(false), // If we can't get dates, assume not temporally close
    }
}

/// Get the most recent article publication date for a cluster
async fn get_most_recent_article_date(db: &Database, cluster_id: i64) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"
        SELECT a.pub_date
        FROM articles a
        JOIN article_cluster_mappings acm ON a.id = acm.article_id
        WHERE acm.cluster_id = ? AND a.pub_date IS NOT NULL
        ORDER BY a.pub_date DESC
        LIMIT 1
        "#,
    )
    .bind(cluster_id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row
        .map(|r| r.get::<Option<String>, _>("pub_date"))
        .flatten())
}

/// Checks for clusters similar to the given cluster and merges them if criteria are met
///
/// This is designed to be lightweight and called during regular article processing
/// to avoid the need for a separate background process.
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to check for similar clusters
/// * `llm_client` - LLM client to use for summary generation
///
/// # Returns
/// * `Ok(Some(new_cluster_id))` - If clusters were merged, the ID of the new cluster
/// * `Ok(None)` - If no clusters were merged
/// * `Err` - If there was an error during the process
pub async fn check_and_merge_similar_clusters(
    db: &Database,
    cluster_id: i64,
    llm_client: &crate::LLMClient,
) -> Result<Option<i64>> {
    // Step 1: Get the primary entities for this cluster
    let row = sqlx::query(
        "SELECT primary_entity_ids FROM article_clusters WHERE id = ? AND status = 'active'",
    )
    .bind(cluster_id)
    .fetch_optional(db.pool())
    .await?;

    let Some(row) = row else {
        // Cluster might have been deleted or already merged
        return Ok(None);
    };

    let primary_entity_ids: String = row.get("primary_entity_ids");
    let entity_ids: HashSet<i64> = serde_json::from_str::<Vec<i64>>(&primary_entity_ids)?
        .into_iter()
        .collect();

    if entity_ids.is_empty() {
        return Ok(None); // No entities to match on
    }

    // Step 2: Find other clusters with sufficient entity overlap
    // We're only checking active clusters with high similarity to the current one
    let similar_clusters = find_similar_clusters(db, cluster_id, &entity_ids, 0.7).await?;

    if similar_clusters.is_empty() {
        return Ok(None);
    }

    // Step 3: For efficiency, only consider merging if we found exactly one similar cluster
    // More complex merges can be handled by the admin CLI tool
    if similar_clusters.len() == 1 {
        let similar_cluster_id = similar_clusters[0];

        // Verify both clusters are recent (within last 14 days)
        if !are_clusters_temporally_close(db, cluster_id, similar_cluster_id, 14).await? {
            return Ok(None);
        }

        // Step 4: Merge the two clusters
        let merge_reason = "Automatic merge due to high entity overlap";
        let merge_ids = vec![cluster_id, similar_cluster_id];
        let new_cluster_id = merge_clusters(db, &merge_ids, Some(merge_reason)).await?;

        // Step 5: Generate a new summary
        let _ = generate_cluster_summary(db, llm_client, new_cluster_id).await?;

        // Step 6: Update the significance score
        let _ = calculate_cluster_significance(db, new_cluster_id).await?;

        return Ok(Some(new_cluster_id));
    }

    Ok(None)
}

/// Find clusters with similar entity profiles to the given cluster
async fn find_similar_clusters(
    db: &Database,
    current_cluster_id: i64,
    current_entities: &HashSet<i64>,
    min_similarity: f64,
) -> Result<Vec<i64>> {
    // Get all active clusters except the current one
    let rows = sqlx::query(
        "SELECT id, primary_entity_ids FROM article_clusters WHERE id != ? AND status = 'active'",
    )
    .bind(current_cluster_id)
    .fetch_all(db.pool())
    .await?;

    let mut similar_clusters = Vec::new();

    for row in rows {
        let id: i64 = row.get("id");
        let entities_json: String = row.get("primary_entity_ids");

        let entities: HashSet<i64> = serde_json::from_str::<Vec<i64>>(&entities_json)?
            .into_iter()
            .collect();

        if entities.is_empty() {
            continue;
        }

        // Calculate Jaccard similarity
        let intersection = current_entities.intersection(&entities).count();
        let union = current_entities.union(&entities).count();

        let similarity = intersection as f64 / union as f64;

        if similarity >= min_similarity {
            debug!(
                "Clusters {} and {} have entity similarity: {:.4}",
                current_cluster_id, id, similarity
            );
            similar_clusters.push(id);
        }
    }

    Ok(similar_clusters)
}

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
