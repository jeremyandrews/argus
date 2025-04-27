use anyhow::Result;
use argus::db::Database;
use rand::Rng;
use std::env;
use std::fs::File;
use std::io::Write;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Utility to create a CSV file with potential article match pairs for analysis.
///
/// This tool generates article pairs based on specified criteria or random sampling,
/// which can then be used with the batch_analyze tool to evaluate matching.
///
/// Usage:
///    cargo run --bin create_match_pairs -- OUTPUT_CSV [NUM_PAIRS] [DAYS_BACK]
///
/// Example:
///    cargo run --bin create_match_pairs -- match_pairs.csv 100 7
///    
/// This will create a CSV file with 100 article pairs from the last 7 days.

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    // Get command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 4 {
        eprintln!("Usage: {} OUTPUT_CSV [NUM_PAIRS] [DAYS_BACK]", args[0]);
        eprintln!("Example: {} match_pairs.csv 100 7", args[0]);
        std::process::exit(1);
    }

    let output_file = &args[1];
    let num_pairs: usize = args.get(2).map_or(100, |s| s.parse().unwrap_or(100));
    let days_back: i64 = args.get(3).map_or(7, |s| s.parse().unwrap_or(7));

    // Get database instance
    let db = Database::instance().await;

    // Get recent articles with entities
    info!(
        "Finding articles from the last {} days with extracted entities...",
        days_back
    );
    let date_threshold = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(days_back))
        .unwrap_or_else(|| chrono::Utc::now())
        .to_rfc3339();

    let articles = db.find_articles_with_entities(&date_threshold).await?;
    info!(
        "Found {} articles with entities from the past {} days",
        articles.len(),
        days_back
    );

    // Create pairs of articles
    let mut pairs = Vec::new();

    // First create some pairs of articles from the same day, which are more likely to match
    let articles_by_date = group_articles_by_date(&articles);

    info!("Creating pairs from articles published on the same day...");
    for (_date, article_ids) in articles_by_date.iter() {
        if article_ids.len() < 2 {
            continue;
        }

        // Pick N random pairs from each day with more than one article
        let mut day_pairs = 0;
        let target_pairs_per_day =
            std::cmp::min(5, (article_ids.len() * (article_ids.len() - 1)) / 2);

        for i in 0..article_ids.len() {
            if day_pairs >= target_pairs_per_day {
                break;
            }

            for j in (i + 1)..article_ids.len() {
                if day_pairs >= target_pairs_per_day {
                    break;
                }

                pairs.push((article_ids[i], article_ids[j]));
                day_pairs += 1;
            }
        }
    }

    info!("Created {} same-day article pairs", pairs.len());

    // Then add random pairs until we reach the target number
    info!(
        "Adding random article pairs to reach target of {} pairs...",
        num_pairs
    );
    let mut rng = rand::rng();

    while pairs.len() < num_pairs && articles.len() >= 2 {
        // Generate random indices
        let source_idx = rng.random_range(0..articles.len());
        let mut target_idx = rng.random_range(0..articles.len());

        // Make sure we don't get the same article
        while source_idx == target_idx {
            target_idx = rng.random_range(0..articles.len());
        }

        let source_id = articles[source_idx];
        let target_id = articles[target_idx];

        // Make sure we don't have this pair already
        if !pairs.contains(&(source_id, target_id)) && !pairs.contains(&(target_id, source_id)) {
            pairs.push((source_id, target_id));
        }
    }

    info!("Final pairs count: {}", pairs.len());

    // Write to CSV file
    let mut file = File::create(output_file)?;
    writeln!(file, "source_id,target_id")?;

    for &(source_id, target_id) in &pairs {
        writeln!(file, "{},{}", source_id, target_id)?;
    }

    info!(
        "Successfully wrote {} article pairs to {}",
        pairs.len(),
        output_file
    );
    info!(
        "To analyze these pairs, run: cargo run --bin batch_analyze -- {}",
        output_file
    );

    Ok(())
}

fn group_articles_by_date(articles: &[i64]) -> std::collections::HashMap<String, Vec<i64>> {
    let mut by_date = std::collections::HashMap::new();

    // This is a simplified version. In a real implementation, we would
    // query the database to get the publication dates for these articles.
    // For now, we'll just group them randomly.

    for &article_id in articles {
        // Simulate a date by using the article_id modulo 10
        let fake_date = format!("2025-04-{:02}", article_id % 10 + 21);
        by_date
            .entry(fake_date)
            .or_insert_with(Vec::new)
            .push(article_id);
    }

    by_date
}
