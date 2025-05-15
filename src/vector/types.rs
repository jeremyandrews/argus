use serde::Serialize;

/// Represents a matched article with similarity scores
#[derive(Debug, Serialize)]
pub struct ArticleMatch {
    // Basic article identification and metadata
    pub id: i64, // Database ID of the article (always included)
    pub published_date: String,
    pub category: String,
    pub quality_score: i8,
    pub score: f32, // final combined similarity score

    // Vector similarity metrics
    pub vector_score: Option<f32>, // Raw vector similarity score
    pub vector_active_dimensions: Option<usize>, // Number of active dimensions in vector
    pub vector_magnitude: Option<f32>, // Vector magnitude

    // Entity similarity metrics
    pub entity_overlap_count: Option<usize>, // Total number of overlapping entities
    pub primary_overlap_count: Option<usize>, // Number of primary entities that overlap
    pub person_overlap: Option<f32>,         // Person similarity score (0-1)
    pub org_overlap: Option<f32>,            // Organization similarity score (0-1)
    pub location_overlap: Option<f32>,       // Location similarity score (0-1)
    pub event_overlap: Option<f32>,          // Event similarity score (0-1)
    pub temporal_proximity: Option<f32>,     // Temporal proximity score (0-1)

    // Formula explanation
    pub similarity_formula: Option<String>, // Explanation of how the score was calculated
}

/// Represents an article match that fell below the threshold (near-miss)
#[derive(Debug, Serialize)]
pub struct NearMissMatch {
    pub article_id: i64,                     // Database ID of the article
    pub score: f32,                          // Final similarity score (below threshold)
    pub threshold: f32,                      // Threshold that was used
    pub missing_score: f32,                  // How much more score was needed to match
    pub vector_score: Option<f32>,           // Vector similarity component
    pub entity_score: Option<f32>,           // Entity similarity component
    pub entity_overlap_count: Option<usize>, // Number of shared entities
    pub reason: String,                      // Human-readable explanation of why it didn't match
}

/// Enhanced article match with both vector and entity similarity
#[derive(Debug)]
pub(crate) struct EnhancedArticleMatch {
    pub article_id: i64,
    pub vector_score: f32,
    pub entity_similarity: crate::entity::EntitySimilarityMetrics,
    pub final_score: f32,
    pub category: String,
    pub published_date: String,
    pub quality_score: i8,
}
