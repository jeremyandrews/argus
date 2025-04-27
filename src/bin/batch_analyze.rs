use anyhow::{Context, Result};
use argus::db::Database;
use argus::entity::matching::calculate_entity_similarity;
use argus::vector::calculate_vector_similarity;
use argus::vector::get_article_entities;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// Batch analysis tool for processing multiple article pairs to identify matching patterns.
///
/// This tool reads a CSV file with article ID pairs and analyzes their match characteristics.
///
/// Usage:
///    cargo run --bin batch_analyze INPUT_CSV [OUTPUT_CSV]
///
/// Example:
///    cargo run --bin batch_analyze data/article_pairs.csv results.csv
///
/// Input CSV format:
///    source_id,target_id,[expected_match]
///    12345,67890,true
///    45678,23456,false
///
/// Output CSV format:
///    source_id,target_id,match_status,combined_score,vector_score,entity_score,shared_entities,primary_shared,reason
///    12345,67890,false,0.62,0.76,0.42,2,1,"Combined score below threshold..."
///    45678,23456,false,0.42,0.51,0.28,1,0,"Weak entity similarity..."
///
/// This allows for large-scale analysis of match patterns and can help identify
/// where the matching algorithm needs improvement.

struct ArticlePair {
    source_id: i64,
    target_id: i64,
    expected_match: Option<bool>,
}

#[derive(Clone, Debug)]
struct MatchResult {
    source_id: i64,
    target_id: i64,
    match_status: bool,
    combined_score: f32,
    vector_score: f32,
    entity_score: f32,
    shared_entities: usize,
    primary_shared: usize,
    reason: String,
    expected_match: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} INPUT_CSV [OUTPUT_CSV]", args[0]);
        eprintln!("Example: {} data/article_pairs.csv results.csv", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];
    let output_file = args
        .get(2)
        .map(|s| s.as_str())
        .unwrap_or("batch_results.csv");

    // Read article pairs from CSV
    info!("Reading article pairs from {}", input_file);
    let pairs = read_article_pairs(input_file)?;
    info!("Loaded {} article pairs for analysis", pairs.len());

    // Analyze each pair
    let mut results = Vec::new();
    for (i, pair) in pairs.iter().enumerate() {
        info!(
            "Analyzing pair {}/{}: {} and {}",
            i + 1,
            pairs.len(),
            pair.source_id,
            pair.target_id
        );
        match analyze_article_pair(pair).await {
            Ok(result) => {
                // Save match status before moving the result
                let match_status = result.match_status;

                // Log expected vs actual outcomes if expectations were provided
                if let Some(expected) = pair.expected_match {
                    if expected != match_status {
                        warn!(
                            "Mismatch between expected ({}) and actual ({}) match status for articles {} and {}",
                            expected, match_status, pair.source_id, pair.target_id
                        );
                    }
                }

                // Push the result after using its fields
                results.push(result);
            }
            Err(e) => {
                error!(
                    "Failed to analyze article pair {} and {}: {}",
                    pair.source_id, pair.target_id, e
                );
            }
        }
    }

    // Write results to CSV
    write_results_to_csv(&results, output_file)?;
    info!("Analysis complete. Results written to {}", output_file);

    // Print summary statistics
    print_summary_statistics(&results);

    Ok(())
}

fn read_article_pairs(input_file: &str) -> Result<Vec<ArticlePair>> {
    let file =
        File::open(input_file).context(format!("Failed to open input file: {}", input_file))?;
    let reader = BufReader::new(file);
    let mut pairs = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line.context(format!("Failed to read line {} from input file", i + 1))?;

        // Skip header line if present
        if i == 0 && line.starts_with("source_id,target_id") {
            continue;
        }

        // Parse CSV line
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 2 {
            warn!("Skipping invalid line {}: {}", i + 1, line);
            continue;
        }

        // Parse IDs
        let source_id = parts[0].trim().parse::<i64>().context(format!(
            "Invalid source_id on line {}: {}",
            i + 1,
            parts[0]
        ))?;
        let target_id = parts[1].trim().parse::<i64>().context(format!(
            "Invalid target_id on line {}: {}",
            i + 1,
            parts[1]
        ))?;

        // Parse expected match if present
        let expected_match = if parts.len() > 2 {
            match parts[2].trim().to_lowercase().as_str() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            }
        } else {
            None
        };

        pairs.push(ArticlePair {
            source_id,
            target_id,
            expected_match,
        });
    }

    Ok(pairs)
}

async fn analyze_article_pair(pair: &ArticlePair) -> Result<MatchResult> {
    let db = Database::instance().await;
    let source_id = pair.source_id;
    let target_id = pair.target_id;

    // Get entities for both articles
    let source_entities = match get_article_entities(source_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            return Err(anyhow::anyhow!(
                "Source article {} has no extracted entities",
                source_id
            ));
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to get entities for source article {}: {}",
                source_id,
                e
            ));
        }
    };

    let target_entities = match get_article_entities(target_id).await {
        Ok(Some(entities)) => entities,
        Ok(None) => {
            return Err(anyhow::anyhow!(
                "Target article {} has no extracted entities",
                target_id
            ));
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to get entities for target article {}: {}",
                target_id,
                e
            ));
        }
    };

    // Get date information
    let (source_pub_date, source_event_date) =
        match db.get_article_details_with_dates(source_id).await {
            Ok(dates) => dates,
            Err(e) => {
                warn!("Failed to get source article dates: {}", e);
                (None, None)
            }
        };

    let (target_pub_date, _) = match db.get_article_details_with_dates(target_id).await {
        Ok(dates) => dates,
        Err(e) => {
            warn!("Failed to get target article dates: {}", e);
            (None, None)
        }
    };

    // Calculate vector similarity
    let dummy_vector = vec![0.0; 1024]; // This is a dummy vector, the calculation will use actual vectors from DB
    let vector_similarity = match calculate_vector_similarity(&dummy_vector, target_id).await {
        Ok(similarity) => similarity,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to calculate vector similarity: {}",
                e
            ));
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
            "No shared entities".to_string()
        } else if vector_similarity < 0.5 {
            format!("Low vector similarity ({:.2})", vector_similarity)
        } else if entity_sim.combined_score < 0.3 {
            format!("Weak entity similarity ({:.2})", entity_sim.combined_score)
        } else {
            format!(
                "Combined score below threshold (needs {:.2} more)",
                missing_score
            )
        }
    } else {
        "Match success".to_string()
    };

    Ok(MatchResult {
        source_id,
        target_id,
        match_status: is_match,
        combined_score,
        vector_score: vector_similarity,
        entity_score: entity_sim.combined_score,
        shared_entities: entity_sim.entity_overlap_count,
        primary_shared: entity_sim.primary_overlap_count,
        reason,
        expected_match: pair.expected_match,
    })
}

fn write_results_to_csv(results: &[MatchResult], output_file: &str) -> Result<()> {
    let path = Path::new(output_file);
    let mut file =
        File::create(path).context(format!("Failed to create output file: {}", output_file))?;

    // Write header
    writeln!(
        file,
        "source_id,target_id,match_status,combined_score,vector_score,entity_score,\
                    shared_entities,primary_shared,person_overlap,org_overlap,location_overlap,\
                    event_overlap,reason,expected_match,expected_vs_actual"
    )?;

    // Write records
    for result in results {
        let expected_vs_actual = match result.expected_match {
            Some(expected) => {
                if expected == result.match_status {
                    "correct"
                } else {
                    "incorrect"
                }
            }
            None => "unknown",
        };

        // Escape commas in the reason field
        let safe_reason = result.reason.replace(',', ";");

        writeln!(
            file,
            "{},{},{},{:.2},{:.2},{:.2},{},{},\"{}\",{},{}",
            result.source_id,
            result.target_id,
            result.match_status,
            result.combined_score,
            result.vector_score,
            result.entity_score,
            result.shared_entities,
            result.primary_shared,
            safe_reason,
            result
                .expected_match
                .map_or("unknown".to_string(), |v| v.to_string()),
            expected_vs_actual
        )?;
    }

    Ok(())
}

fn print_summary_statistics(results: &[MatchResult]) {
    let total = results.len();
    let matched = results.iter().filter(|r| r.match_status).count();
    let not_matched = total - matched;

    println!("\nSummary Statistics:");
    println!("-------------------");
    println!("Total article pairs analyzed: {}", total);
    println!(
        "Matched pairs: {} ({:.1}%)",
        matched,
        (matched as f32 / total as f32) * 100.0
    );
    println!(
        "Non-matched pairs: {} ({:.1}%)",
        not_matched,
        (not_matched as f32 / total as f32) * 100.0
    );

    // Expected matches vs. actual matches (if expected data provided)
    let pairs_with_expectations = results
        .iter()
        .filter(|r| r.expected_match.is_some())
        .count();
    if pairs_with_expectations > 0 {
        let correct_predictions = results
            .iter()
            .filter(|r| r.expected_match.is_some() && r.expected_match.unwrap() == r.match_status)
            .count();

        let incorrect_predictions = pairs_with_expectations - correct_predictions;

        println!(
            "\nPrediction Accuracy (for {} pairs with expectations):",
            pairs_with_expectations
        );
        println!(
            "Correct predictions: {} ({:.1}%)",
            correct_predictions,
            (correct_predictions as f32 / pairs_with_expectations as f32) * 100.0
        );
        println!(
            "Incorrect predictions: {} ({:.1}%)",
            incorrect_predictions,
            (incorrect_predictions as f32 / pairs_with_expectations as f32) * 100.0
        );

        // False positives and negatives
        let false_positives = results
            .iter()
            .filter(|r| {
                r.expected_match.is_some()
                    && r.expected_match.unwrap() == false
                    && r.match_status == true
            })
            .count();

        let false_negatives = results
            .iter()
            .filter(|r| {
                r.expected_match.is_some()
                    && r.expected_match.unwrap() == true
                    && r.match_status == false
            })
            .count();

        println!(
            "False positives: {} ({:.1}% of non-matches)",
            false_positives,
            if pairs_with_expectations > 0 {
                (false_positives as f32 / pairs_with_expectations as f32) * 100.0
            } else {
                0.0
            }
        );
        println!(
            "False negatives: {} ({:.1}% of expected matches)",
            false_negatives,
            if pairs_with_expectations > 0 {
                (false_negatives as f32 / pairs_with_expectations as f32) * 100.0
            } else {
                0.0
            }
        );
    }

    // Reasons for non-matches
    println!("\nReasons for Non-Matches:");
    let mut reason_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for result in results.iter().filter(|r| !r.match_status) {
        let reason_key = if result.reason.starts_with("No shared entities") {
            "No shared entities"
        } else if result.reason.starts_with("Low vector similarity") {
            "Low vector similarity"
        } else if result.reason.starts_with("Weak entity similarity") {
            "Weak entity similarity"
        } else {
            "Combined score below threshold"
        };

        *reason_counts.entry(reason_key).or_insert(0) += 1;
    }

    for (reason, count) in reason_counts.iter() {
        println!(
            "{}: {} ({:.1}% of non-matches)",
            reason,
            count,
            if not_matched > 0 {
                (*count as f32 / not_matched as f32) * 100.0
            } else {
                0.0
            }
        );
    }

    // Score distributions for matches vs non-matches
    println!("\nScore Distributions:");
    if matched > 0 {
        let matched_results: Vec<&MatchResult> =
            results.iter().filter(|r| r.match_status).collect();
        let avg_combined = matched_results
            .iter()
            .map(|r| r.combined_score)
            .sum::<f32>()
            / matched as f32;
        let avg_vector =
            matched_results.iter().map(|r| r.vector_score).sum::<f32>() / matched as f32;
        let avg_entity =
            matched_results.iter().map(|r| r.entity_score).sum::<f32>() / matched as f32;
        let avg_shared = matched_results
            .iter()
            .map(|r| r.shared_entities as f32)
            .sum::<f32>()
            / matched as f32;

        println!("Matched pairs - Avg combined score: {:.2}, Avg vector: {:.2}, Avg entity: {:.2}, Avg shared entities: {:.1}",
                avg_combined, avg_vector, avg_entity, avg_shared);
    }

    if not_matched > 0 {
        let nonmatched_results: Vec<&MatchResult> =
            results.iter().filter(|r| !r.match_status).collect();
        let avg_combined = nonmatched_results
            .iter()
            .map(|r| r.combined_score)
            .sum::<f32>()
            / not_matched as f32;
        let avg_vector = nonmatched_results
            .iter()
            .map(|r| r.vector_score)
            .sum::<f32>()
            / not_matched as f32;
        let avg_entity = nonmatched_results
            .iter()
            .map(|r| r.entity_score)
            .sum::<f32>()
            / not_matched as f32;
        let avg_shared = nonmatched_results
            .iter()
            .map(|r| r.shared_entities as f32)
            .sum::<f32>()
            / not_matched as f32;

        println!("Non-matched pairs - Avg combined score: {:.2}, Avg vector: {:.2}, Avg entity: {:.2}, Avg shared entities: {:.1}",
                avg_combined, avg_vector, avg_entity, avg_shared);
    }
}
