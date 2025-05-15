// Module declarations
pub mod assignment;
pub mod entities;
pub mod merging;
pub mod significance;
pub mod summary;
pub mod temporal;
#[cfg(test)]
mod tests;
pub mod types;
pub mod util;

// Re-export all types from types module for backward compatibility
pub use types::*;

// Re-export key functions from modules for backward compatibility
pub use assignment::assign_article_to_cluster;
pub use entities::{combine_entities_from_clusters, update_cluster_primary_entities};
pub use merging::core::merge_clusters;
pub use merging::history::{
    get_clusters_merged_into, get_merged_cluster_destination, get_merged_clusters,
    mark_cluster_as_merged,
};
pub use merging::similarity::{
    check_and_merge_similar_clusters, find_clusters_with_entity_overlap,
};
pub use significance::calculate_cluster_significance;
pub use summary::{generate_cluster_summary, get_clusters_needing_summary_updates};
pub use util::create_empty_cluster;

/// Minimum similarity score required to assign an article to an existing cluster
pub const MIN_CLUSTER_SIMILARITY: f64 = 0.60;

/// Maximum number of articles to consider when generating a cluster summary
pub const MAX_SUMMARY_ARTICLES: usize = 10;
