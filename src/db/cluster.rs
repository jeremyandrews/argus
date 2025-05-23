use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::{self, Row};
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use tracing::{debug, info};

use crate::clustering::types::{ClusterArticle, EntityDetail};
use crate::db::core::Database;
use crate::entity::types::EntityType;

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
    // Get the article's entities
    let entities = get_article_entities(db, article_id).await?;

    // Skip articles with no primary entities
    if entities.is_empty() {
        debug!("Skipping article {}: no primary entities", article_id);
        return Ok(0);
    }

    // Find the best matching cluster
    let best_match = find_best_matching_cluster(db, &entities).await?;

    let cluster_id = match best_match {
        Some((cluster_id, similarity)) => {
            if similarity >= crate::clustering::MIN_CLUSTER_SIMILARITY {
                // Assign to existing cluster
                info!(
                    "Assigning article {} to existing cluster {} (similarity: {:.4})",
                    article_id, cluster_id, similarity
                );

                assign_to_cluster(db, article_id, cluster_id, similarity).await?
            } else {
                // Create new cluster as the similarity is below threshold
                info!("Creating new cluster for article {}: best match similarity ({:.4}) below threshold", 
                     article_id, similarity);

                create_cluster_for_article(db, article_id, &entities).await?
            }
        }
        None => {
            // No matching clusters found, create a new one
            info!(
                "Creating new cluster for article {}: no existing clusters with matching entities",
                article_id
            );

            create_cluster_for_article(db, article_id, &entities).await?
        }
    };

    // Update the article's cluster_id
    update_article_cluster_id(db, article_id, cluster_id).await?;

    Ok(cluster_id)
}

/// Retrieves the primary entities for an article
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article
///
/// # Returns
/// * `Ok(Vec<i64>)` - Vector of entity IDs that are PRIMARY for this article
/// * `Err` - If there was an error during retrieval
pub async fn get_article_entities(db: &Database, article_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"
        SELECT entity_id FROM article_entities 
        WHERE article_id = ? AND importance = 'PRIMARY'
        "#,
    )
    .bind(article_id)
    .fetch_all(db.pool())
    .await?;

    let entity_ids = rows
        .iter()
        .map(|row| row.get::<i64, _>("entity_id"))
        .collect();

    Ok(entity_ids)
}

/// Finds the best matching cluster for a set of entity IDs
///
/// # Arguments
/// * `db` - Database instance
/// * `entity_ids` - Vector of entity IDs to match against
///
/// # Returns
/// * `Ok(Some((cluster_id, similarity)))` - The best matching cluster and its similarity score
/// * `Ok(None)` - If no matching clusters were found
/// * `Err` - If there was an error during the search
pub async fn find_best_matching_cluster(
    db: &Database,
    entity_ids: &[i64],
) -> Result<Option<(i64, f64)>> {
    if entity_ids.is_empty() {
        return Ok(None);
    }

    // Get all clusters and their primary entities
    let clusters = get_all_clusters(db).await?;

    let mut best_match: Option<(i64, f64)> = None;
    let entity_set: std::collections::HashSet<i64> = entity_ids.iter().cloned().collect();

    for cluster in clusters {
        // Parse the primary_entity_ids JSON array
        let cluster_entities: Vec<i64> = serde_json::from_str(&cluster.primary_entity_ids)?;
        let cluster_entity_set: std::collections::HashSet<i64> =
            cluster_entities.into_iter().collect();

        // Calculate Jaccard similarity: |A ∩ B| / |A ∪ B|
        let intersection = entity_set.intersection(&cluster_entity_set).count();
        let union = entity_set.union(&cluster_entity_set).count();

        if union > 0 {
            let similarity = intersection as f64 / union as f64;

            // Update best match if this is better
            if let Some((_, best_similarity)) = best_match {
                if similarity > best_similarity {
                    best_match = Some((cluster.id, similarity));
                }
            } else {
                best_match = Some((cluster.id, similarity));
            }
        }
    }

    Ok(best_match)
}

/// Gets all existing clusters from the database
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(Vec<ClusterInfo>)` - Vector of cluster information
/// * `Err` - If there was an error during retrieval
pub async fn get_all_clusters(db: &Database) -> Result<Vec<ClusterInfo>> {
    let rows = sqlx::query(
        r#"
        SELECT id, primary_entity_ids FROM article_clusters
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let clusters = rows
        .iter()
        .map(|row| ClusterInfo {
            id: row.get("id"),
            primary_entity_ids: row.get("primary_entity_ids"),
        })
        .collect();

    Ok(clusters)
}

/// Simple struct to hold cluster information during matching
pub struct ClusterInfo {
    pub id: i64,
    pub primary_entity_ids: String,
}

/// Creates a new cluster for an article
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article (not used in this function)
/// * `entity_ids` - Primary entity IDs for the article
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the newly created cluster
/// * `Err` - If there was an error during creation
pub async fn create_cluster_for_article(
    db: &Database,
    _article_id: i64,
    entity_ids: &[i64],
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();
    let primary_entity_ids = serde_json::to_string(entity_ids)?;

    // Create the cluster
    let cluster_id = sqlx::query(
        r#"
        INSERT INTO article_clusters
        (creation_date, last_updated, primary_entity_ids, article_count, needs_summary_update)
        VALUES (?, ?, ?, 1, 1)
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(&primary_entity_ids)
    .execute(db.pool())
    .await?
    .last_insert_rowid();

    debug!(
        "Created new cluster {} with {} primary entities",
        cluster_id,
        entity_ids.len()
    );

    Ok(cluster_id)
}

/// Assigns an article to an existing cluster
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article
/// * `cluster_id` - ID of the cluster to assign to
/// * `similarity_score` - Similarity score between the article and the cluster
///
/// # Returns
/// * `Ok(cluster_id)` - The ID of the cluster the article was assigned to
/// * `Err` - If there was an error during assignment
pub async fn assign_to_cluster(
    db: &Database,
    article_id: i64,
    cluster_id: i64,
    similarity_score: f64,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();

    // Update cluster's last_updated timestamp and increment article count
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET last_updated = ?,
            article_count = article_count + 1,
            needs_summary_update = 1
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    // Create article-cluster mapping
    sqlx::query(
        r#"
        INSERT INTO article_cluster_mappings
        (article_id, cluster_id, added_date, similarity_score)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(article_id)
    .bind(cluster_id)
    .bind(&now)
    .bind(similarity_score)
    .execute(db.pool())
    .await?;

    debug!(
        "Assigned article {} to cluster {} with similarity {:.4}",
        article_id, cluster_id, similarity_score
    );

    Ok(cluster_id)
}

/// Updates an article's cluster_id in the database
///
/// # Arguments
/// * `db` - Database instance
/// * `article_id` - ID of the article
/// * `cluster_id` - ID of the cluster
///
/// # Returns
/// * `Ok(())` - If the update was successful
/// * `Err` - If there was an error during the update
pub async fn update_article_cluster_id(
    db: &Database,
    article_id: i64,
    cluster_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE articles
        SET cluster_id = ?
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .bind(article_id)
    .execute(db.pool())
    .await?;

    Ok(())
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
    let rows = sqlx::query(
        r#"
        SELECT id FROM article_clusters
        WHERE needs_summary_update = 1
        ORDER BY last_updated DESC
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let cluster_ids = rows.iter().map(|row| row.get::<i64, _>("id")).collect();

    Ok(cluster_ids)
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
pub async fn get_cluster_articles(
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
pub async fn get_cluster_entity_details(
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
pub async fn update_cluster_summary(db: &Database, cluster_id: i64, summary: &str) -> Result<()> {
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

/// Gets brief article data for cluster listings (used by API)
///
/// # Arguments  
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster
/// * `limit` - Maximum number of articles to retrieve
///
/// # Returns
/// * `Ok(Vec<ArticleBrief>)` - Brief article data for the cluster
/// * `Err` - If there was an error during retrieval
pub async fn get_cluster_articles_brief(
    db: &Database,
    cluster_id: i64,
    limit: usize,
) -> Result<Vec<crate::app::api::ArticleBrief>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.title, a.url, a.pub_date, acm.similarity_score
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
        let article = crate::app::api::ArticleBrief {
            id: row.get("id"),
            title: row
                .get::<Option<String>, _>("title")
                .unwrap_or_else(|| "Untitled".to_string()),
            url: row.get("url"),
            pub_date: row
                .get::<Option<String>, _>("pub_date")
                .unwrap_or_else(|| "Unknown".to_string()),
            similarity_score: row.get("similarity_score"),
        };

        articles.push(article);
    }

    Ok(articles)
}

/// Gets all active clusters
///
/// # Arguments
/// * `db` - Database instance
///
/// # Returns
/// * `Ok(Vec<ActiveClusterInfo>)` - Vector of active cluster information
/// * `Err` - If there was an error during retrieval
pub async fn get_active_clusters(db: &Database) -> Result<Vec<ActiveClusterInfo>> {
    let rows = sqlx::query(
        r#"
        SELECT id, summary_version, creation_date, last_updated, summary, 
               article_count, importance_score, has_timeline
        FROM article_clusters
        WHERE status = 'active'
        "#,
    )
    .fetch_all(db.pool())
    .await?;

    let mut clusters = Vec::new();

    for row in rows {
        let cluster = ActiveClusterInfo {
            id: row.get("id"),
            summary_version: row.get("summary_version"),
            creation_date: row.get("creation_date"),
            last_updated: row.get("last_updated"),
            summary: row.get("summary"),
            article_count: row.get("article_count"),
            importance_score: row.get("importance_score"),
            has_timeline: row.get::<i32, _>("has_timeline") != 0,
        };

        clusters.push(cluster);
    }

    Ok(clusters)
}

/// Struct to hold active cluster information
pub struct ActiveClusterInfo {
    pub id: i64,
    pub summary_version: i32,
    pub creation_date: String,
    pub last_updated: String,
    pub summary: Option<String>,
    pub article_count: i32,
    pub importance_score: f64,
    pub has_timeline: bool,
}

/// Checks if a cluster exists
///
/// # Arguments
/// * `db` - Database instance
/// * `cluster_id` - ID of the cluster to check
///
/// # Returns
/// * `Ok(bool)` - True if the cluster exists, false otherwise
/// * `Err` - If there was an error during the check
pub async fn does_cluster_exist(db: &Database, cluster_id: i64) -> Result<bool> {
    let row = sqlx::query("SELECT 1 FROM article_clusters WHERE id = ?")
        .bind(cluster_id)
        .fetch_optional(db.pool())
        .await?;

    Ok(row.is_some())
}

/// Calculate and update significance score for a cluster
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

/// Gets the most recent article publication date for a cluster
pub async fn get_most_recent_article_date(
    db: &Database,
    cluster_id: i64,
) -> Result<Option<String>> {
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

/// Creates an empty cluster with default values
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

/// Mark a cluster as merged into another cluster
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

/// Get all clusters that have been merged and their destinations
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

/// Combine all entity IDs from multiple clusters
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
pub async fn update_cluster_primary_entities(
    db: &Database,
    cluster_id: i64,
    entity_ids: &[i64],
) -> Result<()> {
    let entity_ids_json = serde_json::to_string(entity_ids)?;
    let now = chrono::Utc::now().to_rfc3339();

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

/// Updates a cluster's article count
pub async fn update_cluster_article_count(
    db: &Database,
    cluster_id: i64,
    count: i32,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE article_clusters
        SET article_count = ?,
            last_updated = ?
        WHERE id = ?
        "#,
    )
    .bind(count)
    .bind(&now)
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    Ok(())
}
