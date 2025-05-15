//! Entity alias management module
//!
//! This module handles the entity alias system using a database-driven approach.
//! The system has migrated from static hardcoded aliases to a dynamic database system.
//!
//! ## Features
//! - Database-driven alias storage and retrieval
//! - Negative match tracking
//! - Support for multiple alias sources (pattern-based, LLM-generated, user-defined)
//! - Fuzzy matching fallback for runtime decisions
//! - Cache layer for frequently accessed aliases

use super::types::EntityType;
use crate::db::core::Database;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument};

// Common cross-language and spelling variations for pattern-based matching
pub const COMMON_VARIATIONS: &[(&str, &str)] = &[
    ("project", "projekt"),
    ("center", "centre"),
    ("defense", "defence"),
    ("program", "programme"),
    ("color", "colour"),
    ("theater", "theatre"),
    ("organization", "organisation"),
    ("analyzer", "analyser"),
];

// Common patterns for extracting aliases
pub const ALIAS_PATTERNS: &[&str] = &[
    // Entity X, also known as Y - limits entity to 100 chars without newlines, prefers capitalized words
    r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:also\s+)?(?:known|called|referred\s+to)\s+as\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{0,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
    // Entity X (aka/formerly Y) - stricter boundary conditions
    r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])\s+\((?:a\.?k\.?a\.?|formerly|previously|originally|né[e]?)\s+["']?(?P<alias>[^,\.\(\)\n]{2,100}?)["']?\)"#,
    // Y, now known as X
    r#"(?i)["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?,?\s+now\s+(?:known\s+as\s+)?["']?(?P<canonical>[^,\.\(\)\n]{2,100}?)["']?(?:[,\.\)]|$)"#,
    // X, which rebranded as Y
    r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+which\s+(?:rebranded|renamed)\s+(?:itself\s+)?(?:as|to)\s+["']?(?P<alias>[^,\.\(\)\n]{2,100}?)["']?(?:[,\.\)]|$)"#,
    // X (full name Y)
    r#"(?i)(?P<alias>[A-Z][^,\.\(\)\n]{0,20}[^,\.\(\)\s\n])\s+\((?:full\s+name|real\s+name|birth\s+name)\s+["']?(?P<canonical>[^,\.\(\)\n]{2,100}?)["']?\)"#,
    // Company acquisition pattern
    r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+(?:acquired|bought|purchased)\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
    // Person title pattern (more specific for people)
    r#"(?i)(?P<canonical>[A-Z][a-zA-Z\-\'\s]{2,50}),?\s+(?:(?:the|a)\s+)?(?:CEO|founder|president|director|chairman|head|leader)\s+of\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
    // Parent company relationship pattern
    r#"(?i)(?P<canonical>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+is\s+(?:the\s+)?(?:parent|holding)\s+company\s+of\s+["']?(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n])["']?(?:[,\.\)]|$)"#,
    // Founder/created by pattern
    r#"(?i)(?P<alias>[A-Z][^,\.\(\)\n]{1,98}[^,\.\(\)\s\n]),?\s+(?:which|that)\s+was\s+(?:founded|created|started)\s+by\s+["']?(?P<canonical>[A-Z][a-zA-Z\-\'\s]{2,50})["']?(?:[,\.\)]|$)"#,
];

/// Check if two entity names are equivalent according to the alias system
///
/// This is the primary interface for checking name equivalence. It uses
/// the database-driven alias system and falls back to fuzzy matching if needed.
///
/// It handles both directions of equivalence and normalizes names before comparison.
#[instrument(level = "debug", skip(db, name1, name2))]
/// Database check for equivalent names
///
/// This function ONLY checks the database and does not use fuzzy matching.
/// For the full matching functionality with fallbacks, use normalizer.async_names_match instead.
pub async fn db_names_match(
    db: &Database,
    name1: &str,
    name2: &str,
    entity_type: EntityType,
) -> anyhow::Result<bool> {
    // If names are identical, they match
    if name1 == name2 {
        return Ok(true);
    }

    // Normalize for database lookup
    let normalizer = super::normalizer::EntityNormalizer::new();
    let norm1 = normalizer.normalize(name1, entity_type);
    let norm2 = normalizer.normalize(name2, entity_type);

    // If normalized forms are identical, they match
    if norm1 == norm2 {
        return Ok(true);
    }

    // Check database only - no fuzzy matching fallback here
    db.are_names_equivalent(&norm1, &norm2, &entity_type.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {}", e))
}

/// Get the canonical name for an entity (database-driven approach)
///
/// This is the preferred way to get a canonical entity name.
/// It uses the database and falls back to returning the input if unavailable.
#[instrument(level = "debug", skip(db, name))]
pub async fn get_canonical_name(
    db: &Database,
    name: &str,
    entity_type: EntityType,
) -> anyhow::Result<String> {
    let normalizer = super::normalizer::EntityNormalizer::new();
    let normalized = normalizer.normalize(name, entity_type);

    // Try database-driven approach
    match db
        .get_canonical_name(&normalized, &entity_type.to_string())
        .await
    {
        Ok(Some(canonical)) => Ok(canonical),
        Ok(None) => {
            // No canonical form found in database, return the normalized input
            Ok(normalized)
        }
        Err(err) => {
            debug!("Database canonical name lookup failed: {}", err);

            // Return the normalized input name on error but log with anyhow
            Ok(normalized)
        }
    }
}

/// Add a new alias to the database
///
/// This is the preferred way to add a new alias. It handles normalization and
/// validation before adding to the database.
#[instrument(level = "debug", skip(db, entity_id, canonical_name, alias_name))]
pub async fn add_alias(
    db: &Database,
    entity_id: Option<i64>,
    canonical_name: &str,
    alias_name: &str,
    entity_type: EntityType,
    source: &str,
    confidence: f64,
) -> anyhow::Result<i64> {
    let result = db
        .add_entity_alias(
            entity_id,
            canonical_name,
            alias_name,
            &entity_type.to_string(),
            source,
            confidence,
            None, // Use default status (PENDING)
            None, // No approver for automatic additions
        )
        .await?;

    Ok(result)
}

/// Cache entry for alias matches with timestamp for TTL
struct CacheEntry {
    is_match: bool,
    timestamp: Instant,
}

/// Global alias cache to reduce database queries
#[derive(Clone)]
pub struct AliasCache {
    // Key is (normalized_name1, normalized_name2, entity_type_str)
    cache: Arc<DashMap<(String, String, String), CacheEntry>>,
    ttl: Duration,
    max_size: usize,
}

impl AliasCache {
    /// Create a new alias cache with specified TTL and max size
    pub fn new(ttl_seconds: u64, max_size: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
            max_size,
        }
    }

    /// Get a global singleton instance of the cache
    pub fn instance() -> &'static AliasCache {
        use once_cell::sync::Lazy;
        static INSTANCE: Lazy<AliasCache> = Lazy::new(|| {
            // Default: 10 minute TTL, 10,000 max entries
            AliasCache::new(600, 10_000)
        });
        &INSTANCE
    }

    /// Try to get a value from the cache
    pub fn get(&self, name1: &str, name2: &str, entity_type: &str) -> Option<bool> {
        // Create a canonical key with names in sorted order for consistent lookups
        let key = Self::make_key(name1, name2, entity_type);

        // Check if entry exists and is not expired
        if let Some(entry) = self.cache.get(&key) {
            if entry.timestamp.elapsed() < self.ttl {
                return Some(entry.is_match);
            } else {
                // Remove expired entry
                self.cache.remove(&key);
            }
        }
        None
    }

    /// Add or update a value in the cache
    pub fn insert(&self, name1: &str, name2: &str, entity_type: &str, is_match: bool) {
        // Ensure we don't exceed max size by removing random entries if needed
        if self.cache.len() >= self.max_size {
            // Simple strategy: remove ~10% of entries when full
            let to_remove = self.max_size / 10;
            let mut removed = 0;

            let expired_keys: Vec<_> = self
                .cache
                .iter()
                .filter(|entry| entry.timestamp.elapsed() > self.ttl)
                .map(|entry| entry.key().clone())
                .take(to_remove)
                .collect();

            for key in expired_keys {
                self.cache.remove(&key);
                removed += 1;
            }

            // If we still need to remove more, take random entries
            if removed < to_remove {
                let random_keys: Vec<_> = self
                    .cache
                    .iter()
                    .map(|entry| entry.key().clone())
                    .take(to_remove - removed)
                    .collect();

                for key in random_keys {
                    self.cache.remove(&key);
                }
            }
        }

        // Insert new entry
        let key = Self::make_key(name1, name2, entity_type);
        self.cache.insert(
            key,
            CacheEntry {
                is_match,
                timestamp: Instant::now(),
            },
        );
    }

    /// Clear the entire cache
    pub fn clear(&self) {
        self.cache.clear();
        info!("Alias cache cleared");
    }

    /// Make a consistent cache key by ordering name1 and name2 alphabetically
    fn make_key(name1: &str, name2: &str, entity_type: &str) -> (String, String, String) {
        if name1 <= name2 {
            (
                name1.to_string(),
                name2.to_string(),
                entity_type.to_string(),
            )
        } else {
            (
                name2.to_string(),
                name1.to_string(),
                entity_type.to_string(),
            )
        }
    }
}

/// Extract potential aliases from text using patterns
///
/// Uses a set of regex patterns to identify potential aliases in text.
/// Returns a vector of (canonical, alias, entity_type, confidence) tuples.
/// Includes validation to prevent article content from being treated as entities.
#[instrument(level = "debug", skip(text))]
pub fn extract_potential_aliases(
    text: &str,
    entity_type: Option<EntityType>,
) -> Vec<(String, String, EntityType, f64)> {
    use regex::Regex;
    let mut results = Vec::new();

    // Compile the patterns
    let patterns: Vec<Regex> = ALIAS_PATTERNS
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();

    // Maximum allowed length for entity names to prevent matching full paragraphs
    const MAX_ENTITY_LENGTH: usize = 100;

    // Maximum number of words for a valid entity
    const MAX_ENTITY_WORDS: usize = 10;

    // Minimum entity word length - entities typically aren't very short words
    const MIN_WORD_LENGTH: usize = 2;

    // Patterns that suggest passage/article content rather than entity names
    const PASSAGE_INDICATORS: [&str; 10] = [
        " the ", " and ", " that ", " with ", " for ", " this ", " from ", " have ", " are ",
        " they ",
    ];

    // Apply each pattern
    for pattern in patterns {
        for cap in pattern.captures_iter(text) {
            if let (Some(canonical_match), Some(alias_match)) =
                (cap.name("canonical"), cap.name("alias"))
            {
                let canonical = canonical_match.as_str().trim();
                let alias = alias_match.as_str().trim();

                // Skip if canonical and alias are the same or too short
                if canonical == alias || canonical.len() < 2 || alias.len() < 2 {
                    continue;
                }

                // Validate entity length - reject overly long entities
                if canonical.len() > MAX_ENTITY_LENGTH || alias.len() > MAX_ENTITY_LENGTH {
                    debug!(
                        target: super::TARGET_ENTITY,
                        "Rejecting potential alias due to excessive length: '{}' ↔ '{}'",
                        if canonical.len() > 30 { canonical[..27].to_string() + "..." } else { canonical.to_string() },
                        if alias.len() > 30 { alias[..27].to_string() + "..." } else { alias.to_string() }
                    );
                    continue;
                }

                // Validate word count - entities typically don't have many words
                let canonical_words = canonical.split_whitespace().count();
                let alias_words = alias.split_whitespace().count();
                if canonical_words > MAX_ENTITY_WORDS || alias_words > MAX_ENTITY_WORDS {
                    debug!(
                        target: super::TARGET_ENTITY,
                        "Rejecting potential alias due to excessive word count ({}, {}): '{}' ↔ '{}'",
                        canonical_words, alias_words,
                        if canonical.len() > 30 { canonical[..27].to_string() + "..." } else { canonical.to_string() },
                        if alias.len() > 30 { alias[..27].to_string() + "..." } else { alias.to_string() }
                    );
                    continue;
                }

                // Validate that it's not a passage by checking common sentence indicators
                let lowercase_canonical = canonical.to_lowercase();
                let lowercase_alias = alias.to_lowercase();

                if PASSAGE_INDICATORS.iter().any(|&indicator| {
                    lowercase_canonical.contains(indicator) || lowercase_alias.contains(indicator)
                }) {
                    debug!(
                        target: super::TARGET_ENTITY,
                        "Rejecting potential alias that appears to be article content: '{}' ↔ '{}'",
                        if canonical.len() > 30 { canonical[..27].to_string() + "..." } else { canonical.to_string() },
                        if alias.len() > 30 { alias[..27].to_string() + "..." } else { alias.to_string() }
                    );
                    continue;
                }

                // Check for sentence-like structures (multiple words with punctuation)
                if (lowercase_canonical.contains('.') && lowercase_canonical.contains(' '))
                    || (lowercase_alias.contains('.') && lowercase_alias.contains(' '))
                {
                    debug!(
                        target: super::TARGET_ENTITY,
                        "Rejecting potential alias with sentence structure: '{}' ↔ '{}'",
                        if canonical.len() > 30 { canonical[..27].to_string() + "..." } else { canonical.to_string() },
                        if alias.len() > 30 { alias[..27].to_string() + "..." } else { alias.to_string() }
                    );
                    continue;
                }

                // Infer entity type if not provided
                // This is a simplified approach - could be improved with NER
                let inferred_type = entity_type.unwrap_or_else(|| {
                    // Basic rules for inferring type
                    if canonical.chars().next().unwrap_or(' ').is_uppercase() {
                        if canonical.split_whitespace().count() <= 2 {
                            EntityType::Person // Assume short capitalized names are people
                        } else {
                            EntityType::Organization // Assume longer capitalized names are organizations
                        }
                    } else {
                        EntityType::Product // Default to product for lowercase names
                    }
                });

                // Check if words are meaningful enough to be entities
                // Some entities like people and organizations typically have longer words
                if inferred_type == EntityType::Person || inferred_type == EntityType::Organization
                {
                    let has_meaningful_words = canonical
                        .split_whitespace()
                        .any(|word| word.len() >= MIN_WORD_LENGTH)
                        && alias
                            .split_whitespace()
                            .any(|word| word.len() >= MIN_WORD_LENGTH);

                    if !has_meaningful_words {
                        debug!(
                            target: super::TARGET_ENTITY,
                            "Rejecting potential alias with insufficient word length: '{}' ↔ '{}'",
                            canonical, alias
                        );
                        continue;
                    }
                }

                // Assign confidence based on pattern quality and validation
                let confidence = 0.8; // High initial confidence for pattern-based extraction

                results.push((
                    canonical.to_string(),
                    alias.to_string(),
                    inferred_type,
                    confidence,
                ));
            }
        }
    }

    results
}
