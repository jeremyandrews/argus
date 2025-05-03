// Declare submodules
mod analysis;
mod common;
mod decisions;
mod entity;
mod insights;
mod relevance;
mod scoring;
mod summarization;

// Re-export all public functions for backward compatibility
pub use analysis::{critical_analysis_prompt, logical_fallacies_prompt, source_analysis_prompt};
pub use common::*;
pub use decisions::{
    city_threat_prompt, confirm_prompt, confirm_threat_prompt, filter_promotional_content,
    is_this_about, region_threat_prompt, threat_prompt,
};
pub use entity::entity_extraction_prompt;
pub use insights::{
    action_recommendations_prompt, additional_insights_prompt, talking_points_prompt,
};
pub use relevance::{
    how_does_it_affect_prompt, relation_to_topic_prompt, threat_locations, why_not_affect_prompt,
};
pub use scoring::{argument_quality_prompt, source_type_prompt, sources_quality_prompt};
pub use summarization::{summary_prompt, tiny_summary_prompt, tiny_title_prompt};
