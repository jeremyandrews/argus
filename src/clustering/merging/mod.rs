pub mod core;
pub mod history;
pub mod similarity;

// Re-export key functions for convenience
pub use core::merge_clusters;
pub use history::{
    get_clusters_merged_into, get_merged_cluster_destination, get_merged_clusters,
    mark_cluster_as_merged,
};
pub use similarity::{check_and_merge_similar_clusters, find_clusters_with_entity_overlap};
