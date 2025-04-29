use anyhow::{Context, Result};
use argus::db::Database;
use argus::entity::matching::calculate_entity_similarity;
use argus::{
    calculate_direct_similarity, get_article_vector_from_qdrant, vector::get_article_entities,
};
use std::env;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

/// Command line utility to analyze why specific article pairs match or don't match.
///
/// Usage:
///    cargo run --bin analyze_matches SOURCE_ID TARGET_ID
///    cargo run --bin analyze_matches -- 12345 67890
///
/// Example:
///    cargo run --bin analyze_matches -- 12345 67890
///
/// This will:
/// 1. Get entity data for both articles
/// 2. Calculate vector similarity between them
/// 3. Calculate entity similarity between them
/// 4. Show the combined score and explain why they did or didn't match
///
/// Output format:
/// ```
/// Article Pair Analysis: 12345 and 67890
/// --------------------------------------
/// Source article has 8 entities (3 PRIMARY)
/// Target article has 6 entities (2 PRIMARY)
/// Shared entities: 2 (1 PRIMARY)
///
/// Vector similarity: 0.76
/// Entity similarity: 0.42
/// Combined score: 0.62 (threshold is 0.75)
///
/// RESULT: NO MATCH (0.62 < 0.75)
/// Reason: Combined score below threshold. Needs 0.13 more points to match.
/// ```

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} SOURCE_ARTICLE_ID TARGET_ARTICLE_ID", args[0]);
        eprintln!("Example: {} 12345 67890", args[0]);
        std::process::exit(1);
    }

    // Parse article IDs
    let source_id: i64 = args[1]
        .parse()
        .context("Failed to parse source article ID as a number")?;
    let target_id: i64 = args[2]
        .parse()
        .context("Failed to parse target article ID as a number")?;

    info!(
        "Analyzing match between articles {} and {}",
        source_id, target_id
    );

    // Get database instance
    let db = Database::instance().await;

    // Get entities for both articles
    let source_entities = match get_article_entities(source_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            println!("Source article {} has no extracted entities", source_id);
            return Ok(());
        }
        Err(e) => {
            error!(
                "Failed to get entities for source article {}: {}",
                source_id, e
            );
            return Err(e.into());
        }
    };

    let target_entities = match get_article_entities(target_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            println!("Target article {} has no extracted entities", target_id);
            return Ok(());
        }
        Err(e) => {
            error!(
                "Failed to get entities for target article {}: {}",
                target_id, e
            );
            return Err(e.into());
        }
    };

    // Get date information
    let (source_pub_date, source_event_date) =
        match db.get_article_details_with_dates(source_id).await {
            Ok(dates) => dates,
            Err(e) => {
                error!("Failed to get source article dates: {}", e);
                (None, None)
            }
        };

    let (target_pub_date, _) = match db.get_article_details_with_dates(target_id).await {
        Ok(dates) => dates,
        Err(e) => {
            error!("Failed to get target article dates: {}", e);
            (None, None)
        }
    };

    // Calculate vector similarity using actual vectors
    let source_vector = match get_article_vector_from_qdrant(source_id).await {
        Ok(vector) => vector,
        Err(e) => {
            error!(
                "Failed to get vector for source article {}: {}",
                source_id, e
            );
            return Err(e.into());
        }
    };

    let target_vector = match get_article_vector_from_qdrant(target_id).await {
        Ok(vector) => vector,
        Err(e) => {
            error!(
                "Failed to get vector for target article {}: {}",
                target_id, e
            );
            return Err(e.into());
        }
    };

    let vector_similarity = match calculate_direct_similarity(&source_vector, &target_vector) {
        Ok(similarity) => similarity,
        Err(e) => {
            error!("Failed to calculate vector similarity: {}", e);
            return Err(e.into());
        }
    };

    // Calculate entity similarity
    let entity_sim = calculate_entity_similarity(
        &source_entities,
        &target_entities,
        source_event_date.as_deref().or(source_pub_date.as_deref()),
        target_pub_date.as_deref(),
    );

    // Calculate combined score (60% vector + 40% entity)
    let combined_score = 0.6 * vector_similarity + 0.4 * entity_sim.combined_score;
    let threshold = 0.75;
    let is_match = combined_score >= threshold;

    // Format reason for match or non-match
    let reason = if !is_match {
        let missing_score = threshold - combined_score;

        if entity_sim.entity_overlap_count == 0 {
            "No shared entities. Articles with no entity overlap cannot match regardless of vector similarity.".to_string()
        } else if vector_similarity < 0.5 {
            format!("Low vector similarity ({:.2}). Articles need at least {:.2} more points to reach threshold.", 
                    vector_similarity, missing_score)
        } else if entity_sim.combined_score < 0.3 {
            format!("Weak entity similarity ({:.2}). Despite sharing {} entities, the importance levels or entity types don't align well.",
                    entity_sim.combined_score, entity_sim.entity_overlap_count)
        } else {
            format!(
                "Combined score below threshold. Needs {:.2} more points to reach threshold.",
                missing_score
            )
        }
    } else {
        "Articles match successfully!".to_string()
    };

    // Count primary entities
    let source_primary_count = source_entities
        .entities
        .iter()
        .filter(|e| e.importance == argus::entity::ImportanceLevel::Primary)
        .count();
    let target_primary_count = target_entities
        .entities
        .iter()
        .filter(|e| e.importance == argus::entity::ImportanceLevel::Primary)
        .count();

    // Print detailed report
    println!("\nArticle Pair Analysis: {} and {}", source_id, target_id);
    println!("--------------------------------------");
    println!(
        "Source article has {} entities ({} PRIMARY)",
        source_entities.entities.len(),
        source_primary_count
    );
    println!(
        "Target article has {} entities ({} PRIMARY)",
        target_entities.entities.len(),
        target_primary_count
    );
    println!(
        "Shared entities: {} ({} PRIMARY)",
        entity_sim.entity_overlap_count, entity_sim.primary_overlap_count
    );
    println!();
    println!("Vector similarity: {:.2}", vector_similarity);
    println!("Entity similarity: {:.2}", entity_sim.combined_score);
    println!(
        "Combined score: {:.2} (threshold is {:.2})",
        combined_score, threshold
    );
    println!();
    println!(
        "RESULT: {} ({:.2} {} {:.2})",
        if is_match { "MATCH" } else { "NO MATCH" },
        combined_score,
        if is_match { ">=" } else { "<" },
        threshold
    );
    println!("Reason: {}", reason);

    // Print detailed entity similarity breakdown
    println!("\nDetailed Entity Similarity Breakdown:");
    println!("Person overlap: {:.2}", entity_sim.person_overlap);
    println!(
        "Organization overlap: {:.2}",
        entity_sim.organization_overlap
    );
    println!("Location overlap: {:.2}", entity_sim.location_overlap);
    println!("Event overlap: {:.2}", entity_sim.event_overlap);
    println!("Temporal proximity: {:.2}", entity_sim.temporal_proximity);

    // Print entity lists to help understand the matches
    println!("\nSource Entities:");
    for entity in &source_entities.entities {
        println!(
            "  - {} ({:?}, {:?})",
            entity.name, entity.entity_type, entity.importance
        );
    }

    println!("\nTarget Entities:");
    for entity in &target_entities.entities {
        println!(
            "  - {} ({:?}, {:?})",
            entity.name, entity.entity_type, entity.importance
        );
    }

    // Print shared entities
    println!("\nShared Entities:");
    let source_entity_names: std::collections::HashSet<_> = source_entities
        .entities
        .iter()
        .map(|e| e.normalized_name.as_str())
        .collect();

    for entity in &target_entities.entities {
        if source_entity_names.contains(entity.normalized_name.as_str()) {
            println!(
                "  - {} ({:?}, {:?})",
                entity.name, entity.entity_type, entity.importance
            );
        }
    }

    Ok(())
}
