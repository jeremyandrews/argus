use lazy_static::lazy_static;
use rust_stemmers::{Algorithm, Stemmer};
use std::collections::{HashMap, HashSet};
use strsim::{jaro_winkler, levenshtein};
use tracing::debug;
use unicode_normalization::UnicodeNormalization;
use whatlang::detect as detect_language;

use super::aliases::{self, get_aliases_for_type, COMMON_VARIATIONS};
use super::types::EntityType;
use super::TARGET_ENTITY;
use crate::db::Database;

// Thresholds for different entity types
const PERSON_THRESHOLD: f64 = 0.90;
const ORG_THRESHOLD: f64 = 0.85;
const LOCATION_THRESHOLD: f64 = 0.85;
const PRODUCT_THRESHOLD: f64 = 0.80;
const DEFAULT_THRESHOLD: f64 = 0.85;

// Levenshtein thresholds (max edit distance allowed)
const PERSON_LEVENSHTEIN: usize = 2;
const ORG_LEVENSHTEIN: usize = 3;
const LOCATION_LEVENSHTEIN: usize = 3;
const PRODUCT_LEVENSHTEIN: usize = 3;
const DEFAULT_LEVENSHTEIN: usize = 2;

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

    /// Helper method to determine appropriate similarity threshold by entity type
    pub fn similarity_threshold(&self, entity_type: EntityType) -> f64 {
        *self
            .similarity_thresholds
            .get(&entity_type)
            .unwrap_or(&DEFAULT_THRESHOLD)
    }

    /// Helper method to determine appropriate Levenshtein threshold by entity type
    pub fn levenshtein_threshold(&self, entity_type: EntityType) -> usize {
        match entity_type {
            EntityType::Person => PERSON_LEVENSHTEIN,
            EntityType::Organization => ORG_LEVENSHTEIN,
            EntityType::Location => LOCATION_LEVENSHTEIN,
            EntityType::Product => PRODUCT_LEVENSHTEIN,
            _ => DEFAULT_LEVENSHTEIN,
        }
    }

    /// Normalize entity name based on its type
    pub fn normalize(&self, name: &str, entity_type: EntityType) -> String {
        // Apply basic normalization first
        let mut normalized = self.basic_normalize(name);

        // Apply stemming for certain entity types
        if entity_type == EntityType::Product || entity_type == EntityType::Organization {
            // Use stemming to handle plurals and other variations
            let en_stemmer = Stemmer::create(Algorithm::English);
            normalized = normalized
                .split_whitespace()
                .map(|token| en_stemmer.stem(token).to_string())
                .collect::<Vec<_>>()
                .join(" ");

            debug!(
                target: TARGET_ENTITY,
                "Applied stemming to '{}' resulting in '{}'", name, normalized
            );
        }

        // Look up in alias dictionary
        if let Some(aliases) = ALIAS_MAPS.get(&entity_type) {
            if let Some(canonical) = aliases.get(&normalized) {
                debug!(
                    target: TARGET_ENTITY,
                    "Normalized '{}' to '{}' using aliases", normalized, canonical
                );
                return canonical.clone();
            }
        }

        // Apply common variations for certain entity types
        if entity_type == EntityType::Product || entity_type == EntityType::Organization {
            for (variant, canonical) in COMMON_VARIATIONS {
                if normalized.contains(variant) {
                    let result = normalized.replace(variant, canonical);
                    debug!(
                        target: TARGET_ENTITY,
                        "Normalized '{}' to '{}' using common variations", normalized, result
                    );
                    return result;
                }
            }
        }

        normalized
    }

    /// Determine if two entity names match (async database-backed version)
    ///
    /// This version first tries the database-backed alias system, then falls
    /// back to the original fuzzy matching logic if needed.
    pub async fn async_names_match(
        &self,
        db: &Database,
        name1: &str,
        name2: &str,
        entity_type: EntityType,
    ) -> anyhow::Result<bool> {
        // Try exact match after normalization
        let norm1 = self.normalize(name1, entity_type);
        let norm2 = self.normalize(name2, entity_type);

        if norm1 == norm2 {
            debug!(
                target: TARGET_ENTITY,
                "Exact match after normalization: '{}' == '{}'", name1, name2
            );
            return Ok(true);
        }

        // Try database-driven approach
        match aliases::names_match(db, name1, name2, entity_type).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                debug!(
                    target: TARGET_ENTITY,
                    "Database alias check failed, falling back to fuzzy matching: {}", err
                );
                // Fall through to fuzzy matching
            }
        }

        // Fall back to the original fuzzy matching logic
        Ok(self.fuzzy_match(&norm1, &norm2, name1, name2, entity_type))
    }

    /// Determine if two entity names match (synchronous version)
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

        // Delegate to fuzzy matching helper
        self.fuzzy_match(&norm1, &norm2, name1, name2, entity_type)
    }

    /// Helper method for fuzzy matching
    fn fuzzy_match(
        &self,
        norm1: &str,
        norm2: &str,
        name1: &str,
        name2: &str,
        entity_type: EntityType,
    ) -> bool {
        // For product/organization entities, check for substring containment
        if entity_type == EntityType::Product || entity_type == EntityType::Organization {
            // Check if shorter name is contained in longer name (case insensitive)
            let (shorter, longer) = if norm1.len() < norm2.len() {
                (&norm1, &norm2)
            } else {
                (&norm2, &norm1)
            };

            // Special case for acronyms (all uppercase with no spaces)
            let is_acronym =
                shorter.chars().all(|c| c.is_ascii_uppercase()) && !shorter.contains(' ');

            if longer.contains(shorter) {
                // Verify with token-based match to avoid false positives
                let shorter_tokens: HashSet<_> = shorter.split_whitespace().collect();
                let longer_tokens: HashSet<_> = longer.split_whitespace().collect();

                // Skip very short single-token matches unless it's an acronym
                if shorter_tokens.len() == 1 && shorter.len() < 4 && !is_acronym {
                    // Don't match very short single tokens (avoid "App" matching "Apple")
                    debug!(
                        target: TARGET_ENTITY,
                        "Skipping very short single token match: '{}' in '{}'",
                        shorter, longer
                    );
                }
                // Special case for acronyms
                else if is_acronym {
                    // For acronyms, check if it matches the first letters of words in the longer name
                    let longer_initials: String = longer
                        .split_whitespace()
                        .map(|word| word.chars().next().unwrap_or(' '))
                        .collect();

                    if longer_initials.contains(shorter) {
                        debug!(
                            target: TARGET_ENTITY,
                            "Acronym match: '{}' maps to initials in '{}'",
                            shorter, longer
                        );
                        return true;
                    }

                    // Also check if acronym appears as a standalone token
                    if longer_tokens.iter().any(|token| token == shorter) {
                        debug!(
                            target: TARGET_ENTITY,
                            "Acronym token match: '{}' found in tokens of '{}'",
                            shorter, longer
                        );
                        return true;
                    }
                }
                // Require all tokens from shorter name to exist in longer name
                else if shorter_tokens.is_subset(&longer_tokens) {
                    debug!(
                        target: TARGET_ENTITY,
                        "Substring match: '{}' contained in '{}' with token verification",
                        shorter, longer
                    );
                    return true;
                }
            }
        }

        // Try fuzzy matching if enabled
        if self.use_fuzzy_matching {
            // First try Jaro-Winkler (better for names)
            let jw_threshold = self.similarity_threshold(entity_type);
            let jw_similarity = jaro_winkler(&norm1, &norm2);

            if jw_similarity >= jw_threshold {
                debug!(
                    target: TARGET_ENTITY,
                    "Jaro-Winkler match: '{}' and '{}' with similarity {:.3} (threshold: {:.3})",
                    name1, name2, jw_similarity, jw_threshold
                );
                return true;
            }

            // Then try Levenshtein distance (edit distance)
            let lev_threshold = self.levenshtein_threshold(entity_type);
            let lev_distance = levenshtein(&norm1, &norm2);

            // Only apply Levenshtein for similar-length strings and product/person types
            let length_diff = (norm1.len() as isize - norm2.len() as isize).abs() as usize;
            if length_diff <= lev_threshold && lev_distance <= lev_threshold {
                // Special case for product numbers (e.g., "365" vs "356")
                let is_product_number =
                    norm1.chars().any(|c| c.is_numeric()) && norm2.chars().any(|c| c.is_numeric());

                // For longer strings, require additional constraints
                let max_len = std::cmp::max(norm1.len(), norm2.len());

                // Additional constraints for long strings
                if max_len > 15 && !is_product_number {
                    // For long strings, check for a common prefix
                    let common_prefix_len = norm1
                        .chars()
                        .zip(norm2.chars())
                        .take_while(|(a, b)| a == b)
                        .count();

                    // Only match if they share a common prefix (at least 30%)
                    if common_prefix_len >= (max_len / 3) {
                        debug!(
                            target: TARGET_ENTITY,
                            "Levenshtein match with common prefix: '{}' and '{}' with distance {} (threshold: {})",
                            name1, name2, lev_distance, lev_threshold
                        );
                        return true;
                    }
                } else {
                    // For shorter strings or product numbers, use regular Levenshtein
                    debug!(
                        target: TARGET_ENTITY,
                        "Levenshtein match: '{}' and '{}' with distance {} (threshold: {})",
                        name1, name2, lev_distance, lev_threshold
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Apply basic normalization: Unicode normalization, lowercase, whitespace
    fn basic_normalize(&self, name: &str) -> String {
        // Enhanced apostrophe handling
        let name_without_apostrophes = name
            .replace("'s ", " ") // Remove possessive "'s "
            .replace("'s", "") // Remove possessive at end of word
            .replace("s' ", "s ") // Handle plural possessive
            .replace("' ", " ") // Remove other apostrophe forms
            .replace("'", ""); // Remove remaining apostrophes

        name_without_apostrophes
            .nfkd() // Unicode normalization
            .collect::<String>()
            .to_lowercase() // Case normalization
            .trim() // Remove leading/trailing whitespace
            .replace(|c: char| !c.is_alphanumeric() && c != ' ', " ") // Replace punctuation with space
            .split_whitespace() // Split by whitespace
            .collect::<Vec<_>>()
            .join(" ") // Join with single spaces
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
        assert_eq!(normalizer.basic_normalize("SpaceX's"), "spacex");
        assert_eq!(normalizer.basic_normalize("James' Book"), "james book");

        // Test matching with apostrophes
        assert!(normalizer.names_match("SpaceX's Starlinks", "Starlink", EntityType::Product));
        assert!(normalizer.names_match("McDonald's", "McDonalds", EntityType::Organization));
    }

    #[test]
    fn test_substring_matching() {
        let normalizer = EntityNormalizer::new();

        // Test substring matching for products
        assert!(normalizer.names_match(
            "Atlas V",
            "United Launch Alliance Atlas V rocket",
            EntityType::Product
        ));
        assert!(normalizer.names_match("iPhone", "Apple iPhone 15", EntityType::Product));
        assert!(normalizer.names_match(
            "Starlink",
            "SpaceX Starlink satellites",
            EntityType::Product
        ));

        // Test substring matching for organizations
        assert!(normalizer.names_match(
            "NASA",
            "NASA Goddard Space Flight Center",
            EntityType::Organization
        ));
        assert!(normalizer.names_match(
            "Microsoft",
            "Microsoft Corporation",
            EntityType::Organization
        ));

        // Ensure substring matching only works for Products and Organizations
        assert!(!normalizer.names_match("John", "John Doe Smith", EntityType::Person));
        assert!(!normalizer.names_match("New York", "New York City", EntityType::Location));

        // Test token-based verification (not just substring)
        assert!(!normalizer.names_match("App", "Apple", EntityType::Organization));
        assert!(!normalizer.names_match("Space", "SpaceX", EntityType::Organization));
    }

    #[test]
    fn test_stemming() {
        let normalizer = EntityNormalizer::new();

        // Test stemming for products
        assert!(normalizer.names_match("Rockets", "Rocket", EntityType::Product));
        assert!(normalizer.names_match("Satellites", "Satellite", EntityType::Product));
        assert!(normalizer.names_match("Apple iPhones", "Apple iPhone", EntityType::Product));

        // Test stemming for organizations
        assert!(normalizer.names_match(
            "Microsoft Engineers",
            "Microsoft Engineering",
            EntityType::Organization
        ));
        assert!(normalizer.names_match(
            "Producers Guild",
            "Producer Guild",
            EntityType::Organization
        ));

        // Stemming shouldn't affect other entity types significantly
        assert!(!normalizer.names_match("Americans", "American", EntityType::Person));
    }

    #[test]
    fn test_levenshtein_distance() {
        let normalizer = EntityNormalizer::new();

        // Test Levenshtein distance matching for products (threshold = 3)
        assert!(normalizer.names_match("Microsoft 356", "Microsoft 365", EntityType::Product));
        assert!(normalizer.names_match("MagSafe Chargr", "MagSafe Charger", EntityType::Product));

        // Test Levenshtein distance matching for persons (threshold = 2)
        assert!(normalizer.names_match("Elon Muskk", "Elon Musk", EntityType::Person));
        assert!(normalizer.names_match("Tim Coook", "Tim Cook", EntityType::Person));

        // Test cases that should be beyond threshold
        assert!(!normalizer.names_match(
            "Microsoft Windows",
            "Microsoft Office",
            EntityType::Product
        ));
        assert!(!normalizer.names_match("Joe Biden", "Joe Smith", EntityType::Person));
    }
}
