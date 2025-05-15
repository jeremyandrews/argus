use crate::entity::types::EntityType;
use serde::{Deserialize, Serialize};

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
