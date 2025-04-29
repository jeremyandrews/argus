use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Entity type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Event,
    Product,
    Date,
    Other,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityType::Person => write!(f, "PERSON"),
            EntityType::Organization => write!(f, "ORGANIZATION"),
            EntityType::Location => write!(f, "LOCATION"),
            EntityType::Event => write!(f, "EVENT"),
            EntityType::Product => write!(f, "PRODUCT"),
            EntityType::Date => write!(f, "DATE"),
            EntityType::Other => write!(f, "OTHER"),
        }
    }
}

impl From<&str> for EntityType {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PERSON" => EntityType::Person,
            "ORGANIZATION" => EntityType::Organization,
            "LOCATION" => EntityType::Location,
            "EVENT" => EntityType::Event,
            "PRODUCT" => EntityType::Product,
            "DATE" => EntityType::Date,
            _ => EntityType::Other,
        }
    }
}

/// Entity importance level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImportanceLevel {
    Primary,
    Secondary,
    Mentioned,
}

impl fmt::Display for ImportanceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportanceLevel::Primary => write!(f, "PRIMARY"),
            ImportanceLevel::Secondary => write!(f, "SECONDARY"),
            ImportanceLevel::Mentioned => write!(f, "MENTIONED"),
        }
    }
}

impl From<&str> for ImportanceLevel {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "PRIMARY" => ImportanceLevel::Primary,
            "SECONDARY" => ImportanceLevel::Secondary,
            _ => ImportanceLevel::Mentioned,
        }
    }
}

/// Base entity struct that represents a named entity extracted from an article
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    // Database ID (if stored)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,

    // Original entity name as extracted from text
    pub name: String,

    // Standardized, lowercase name for matching
    pub normalized_name: String,

    // Entity type (person, org, location, etc.)
    pub entity_type: EntityType,

    // How important this entity is to the article
    pub importance: ImportanceLevel,

    // Additional metadata specific to this entity type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl Entity {
    pub fn new(
        name: &str,
        normalized_name: &str,
        entity_type: EntityType,
        importance: ImportanceLevel,
    ) -> Self {
        Entity {
            id: None,
            name: name.to_string(),
            normalized_name: normalized_name.to_string(),
            entity_type,
            importance,
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_id(mut self, id: i64) -> Self {
        self.id = Some(id);
        self
    }
}

/// Collection of extracted entities from an article
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedEntities {
    // Main event date for the article, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,

    // All extracted entities
    pub entities: Vec<Entity>,
}

impl ExtractedEntities {
    pub fn new() -> Self {
        ExtractedEntities {
            event_date: None,
            entities: Vec::new(),
        }
    }

    pub fn with_event_date(mut self, date: &str) -> Self {
        self.event_date = Some(date.to_string());
        self
    }

    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.push(entity);
    }

    pub fn get_entities_by_type(&self, entity_type: EntityType) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.entity_type == entity_type)
            .collect()
    }

    pub fn get_primary_entities(&self) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.importance == ImportanceLevel::Primary)
            .collect()
    }
}

/// Result of entity-based similarity calculation
#[derive(Debug, Clone, Default)]
pub struct EntitySimilarityMetrics {
    // How many entities overlap between the two articles
    pub entity_overlap_count: usize,

    // How many primary entities overlap
    pub primary_overlap_count: usize,

    // Person similarity
    pub person_overlap: f32,

    // Organization similarity
    pub organization_overlap: f32,

    // Location similarity
    pub location_overlap: f32,

    // Event similarity
    pub event_overlap: f32,

    // Product similarity
    pub product_overlap: f32,

    // Temporal proximity (0 to 1, where 1 is same date)
    pub temporal_proximity: f32,

    // Combined entity similarity score (0 to 1)
    pub combined_score: f32,
}

impl EntitySimilarityMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate combined score from individual components
    pub fn calculate_combined_score(&mut self) {
        // Weighted average of entity-based similarities
        self.combined_score = 0.25 * self.person_overlap
            + 0.2 * self.organization_overlap
            + 0.1 * self.location_overlap
            + 0.1 * self.event_overlap
            + 0.2 * self.product_overlap
            + 0.15 * self.temporal_proximity;
    }
}

/// Complete article similarity result combining vector and entity similarity
#[derive(Debug, Clone)]
pub struct EnhancedSimilarity {
    // Article ID from database
    pub article_id: i64,

    // Vector-based similarity score
    pub vector_score: f32,

    // Entity-based similarity metrics
    pub entity_similarity: EntitySimilarityMetrics,

    // Combined final score
    pub final_score: f32,

    // Original metadata from vector similarity
    pub category: String,
    pub published_date: String,
    pub quality_score: i8,
}

impl EnhancedSimilarity {
    pub fn new(
        article_id: i64,
        vector_score: f32,
        entity_similarity: EntitySimilarityMetrics,
        category: String,
        published_date: String,
        quality_score: i8,
    ) -> Self {
        // Calculate combined score with weighted average:
        // 60% vector similarity + 40% entity similarity
        let final_score = 0.6 * vector_score + 0.4 * entity_similarity.combined_score;

        Self {
            article_id,
            vector_score,
            entity_similarity,
            final_score,
            category,
            published_date,
            quality_score,
        }
    }
}
