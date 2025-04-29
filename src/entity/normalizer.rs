use lazy_static::lazy_static;
use std::collections::HashMap;
use strsim::jaro_winkler;
use tracing::debug;
use unicode_normalization::UnicodeNormalization;
use whatlang::detect as detect_language;

use super::aliases::{get_aliases_for_type, COMMON_VARIATIONS};
use super::types::EntityType;
use super::TARGET_ENTITY;

// Thresholds for different entity types
const PERSON_THRESHOLD: f64 = 0.90;
const ORG_THRESHOLD: f64 = 0.85;
const LOCATION_THRESHOLD: f64 = 0.85;
const PRODUCT_THRESHOLD: f64 = 0.80;
const DEFAULT_THRESHOLD: f64 = 0.85;

lazy_static! {
    static ref ALIAS_MAPS: HashMap<EntityType, HashMap<String, String>> = {
        let mut maps = HashMap::new();

        // Initialize maps for each entity type
        maps.insert(EntityType::Person, get_aliases_for_type(EntityType::Person));
        maps.insert(EntityType::Organization, get_aliases_for_type(EntityType::Organization));
        maps.insert(EntityType::Product, get_aliases_for_type(EntityType::Product));
        maps.insert(EntityType::Location, get_aliases_for_type(EntityType::Location));

        maps
    };
}

pub struct EntityNormalizer {
    // Whether to use fuzzy matching as fallback
    use_fuzzy_matching: bool,
    // Custom threshold overrides
    similarity_thresholds: HashMap<EntityType, f64>,
}

impl Default for EntityNormalizer {
    fn default() -> Self {
        let mut thresholds = HashMap::new();
        thresholds.insert(EntityType::Person, PERSON_THRESHOLD);
        thresholds.insert(EntityType::Organization, ORG_THRESHOLD);
        thresholds.insert(EntityType::Location, LOCATION_THRESHOLD);
        thresholds.insert(EntityType::Product, PRODUCT_THRESHOLD);

        Self {
            use_fuzzy_matching: true,
            similarity_thresholds: thresholds,
        }
    }
}

impl EntityNormalizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fuzzy_matching(mut self, enabled: bool) -> Self {
        self.use_fuzzy_matching = enabled;
        self
    }

    pub fn with_threshold(mut self, entity_type: EntityType, threshold: f64) -> Self {
        self.similarity_thresholds.insert(entity_type, threshold);
        self
    }

    /// Normalize entity name based on its type
    pub fn normalize(&self, name: &str, entity_type: EntityType) -> String {
        // First apply basic normalization
        let basic = self.basic_normalize(name);

        // Look up in alias dictionary
        if let Some(aliases) = ALIAS_MAPS.get(&entity_type) {
            if let Some(canonical) = aliases.get(&basic) {
                debug!(
                    target: TARGET_ENTITY,
                    "Normalized '{}' to '{}' using aliases", basic, canonical
                );
                return canonical.clone();
            }
        }

        // Apply common variations for certain entity types
        if entity_type == EntityType::Product || entity_type == EntityType::Organization {
            for (variant, canonical) in COMMON_VARIATIONS {
                if basic.contains(variant) {
                    let normalized = basic.replace(variant, canonical);
                    debug!(
                        target: TARGET_ENTITY,
                        "Normalized '{}' to '{}' using common variations", basic, normalized
                    );
                    return normalized;
                }
            }
        }

        // Return basic normalization if no special cases apply
        basic
    }

    /// Determine if two entity names match
    pub fn names_match(&self, name1: &str, name2: &str, entity_type: EntityType) -> bool {
        // Try exact match after normalization
        let norm1 = self.normalize(name1, entity_type);
        let norm2 = self.normalize(name2, entity_type);

        if norm1 == norm2 {
            debug!(
                target: TARGET_ENTITY,
                "Exact match after normalization: '{}' == '{}'", name1, name2
            );
            return true;
        }

        // Try fuzzy matching if enabled
        if self.use_fuzzy_matching {
            let threshold = self
                .similarity_thresholds
                .get(&entity_type)
                .copied()
                .unwrap_or(DEFAULT_THRESHOLD);

            let similarity = jaro_winkler(&norm1, &norm2);

            if similarity >= threshold {
                debug!(
                    target: TARGET_ENTITY,
                    "Fuzzy match: '{}' and '{}' with similarity {:.3} (threshold: {:.3})",
                    name1, name2, similarity, threshold
                );
                return true;
            }
        }

        false
    }

    /// Apply basic normalization: Unicode normalization, lowercase, whitespace
    fn basic_normalize(&self, name: &str) -> String {
        // Handle apostrophes specially before general punctuation
        let name_without_apostrophes = name
            .replace("'s ", " ") // Remove possessive "'s "
            .replace("' ", " ") // Remove other apostrophe forms
            .replace("'s", "s") // Handle possessive at end of word
            .replace("'", ""); // Remove remaining apostrophes

        name_without_apostrophes
            .nfkd() // Unicode normalization
            .collect::<String>()
            .to_lowercase() // Case normalization
            .trim() // Remove leading/trailing whitespace
            .replace(|c: char| !c.is_alphanumeric() && c != ' ', " ") // Replace punctuation with space
            .replace("  ", " ") // Normalize multiple spaces
            .trim() // Final trim
            .to_string()
    }

    /// Detect language of text to apply language-specific normalization
    fn _detect_language(&self, text: &str) -> Option<String> {
        detect_language(text).map(|info| info.lang().code().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_normalization() {
        let normalizer = EntityNormalizer::new();
        assert_eq!(normalizer.basic_normalize("Blue Origin"), "blue origin");
        assert_eq!(normalizer.basic_normalize("Blue-Origin"), "blue origin");
        assert_eq!(normalizer.basic_normalize(" BLUE  ORIGIN "), "blue origin");
    }

    #[test]
    fn test_name_matching() {
        let normalizer = EntityNormalizer::new();

        // Exact matches after normalization
        assert!(normalizer.names_match("Project Kuiper", "Projekt Kuiper", EntityType::Product));
        assert!(normalizer.names_match("Blue Origin", "BlueOrigin", EntityType::Organization));

        // Fuzzy matches
        assert!(normalizer.names_match("Jeff Bezos", "Jeffrey Bezos", EntityType::Person));
        assert!(normalizer.names_match("Amazon", "Amazon.com", EntityType::Organization));

        // Non-matches
        assert!(!normalizer.names_match("Blue Origin", "SpaceX", EntityType::Organization));
        assert!(!normalizer.names_match("Jeff Bezos", "Elon Musk", EntityType::Person));
    }

    #[test]
    fn test_apostrophe_handling() {
        let normalizer = EntityNormalizer::new();

        // Test apostrophe normalization
        assert_eq!(
            normalizer.basic_normalize("SpaceX's Starlinks"),
            "spacex starlinks"
        );
        assert_eq!(normalizer.basic_normalize("SpaceX's"), "spacexs");
        assert_eq!(normalizer.basic_normalize("James' Book"), "james book");

        // Test matching with apostrophes
        assert!(normalizer.names_match("SpaceX's Starlinks", "Starlink", EntityType::Product));
        assert!(normalizer.names_match("McDonald's", "McDonalds", EntityType::Organization));
    }
}
