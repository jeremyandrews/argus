use anyhow::Result;
use clap::{Arg, Command as ClapCommand};
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use std::time::Instant;

// Global migration start time for consistent timing across all output
static MIGRATION_START_TIME: OnceLock<Instant> = OnceLock::new();

// Helper function to print with timing information
fn timed_println(message: &str) {
    if let Some(start_time) = MIGRATION_START_TIME.get() {
        let elapsed = start_time.elapsed();
        let minutes = elapsed.as_secs() / 60;
        let seconds = elapsed.as_secs() % 60;
        println!("{:02}:{:02} {}", minutes, seconds, message);
    } else {
        println!("{}", message);
    }
}

fn main() -> Result<()> {
    let matches = ClapCommand::new("migrate_to_postgres_complete")
        .about("Complete PostgreSQL migration handling all tables")
        .arg(
            Arg::new("input")
                .help("SQLite dump file (use 'sqlite3 argus.db .dump' to create)")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("output")
                .help("Output PostgreSQL SQL file")
                .short('o')
                .long("output")
                .default_value("postgres_migration.sql"),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable detailed debugging")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let output_file = matches.get_one::<String>("output").unwrap();
    let debug_mode = matches.get_flag("debug");

    // Initialize timing
    MIGRATION_START_TIME
        .set(Instant::now())
        .expect("Failed to initialize migration timing");

    timed_println("🚀 Starting Complete PostgreSQL Migration...");

    // Read input file
    timed_println(&format!("📖 Reading SQLite dump: {}", input_file));
    let sqlite_dump = fs::read_to_string(input_file)?;

    // Transform SQL
    timed_println("🔄 Transforming SQL for PostgreSQL...");
    let mut postgres_sql = String::new();
    let mut skip_until_semicolon = false;
    let mut line_count = 0;
    let mut transformed_count = 0;
    let mut table_stats = HashMap::new();

    for line in sqlite_dump.lines() {
        line_count += 1;
        let trimmed = line.trim();

        // Skip SQLite-specific statements
        if should_skip_line(trimmed) {
            continue;
        }

        // Skip CREATE statements (we use our own schema)
        if trimmed.starts_with("CREATE TABLE")
            || trimmed.starts_with("CREATE UNIQUE INDEX")
            || trimmed.starts_with("CREATE INDEX")
        {
            skip_until_semicolon = true;
            continue;
        }

        if skip_until_semicolon {
            if trimmed.ends_with(";") {
                skip_until_semicolon = false;
            }
            continue;
        }

        // Process INSERT statements
        if trimmed.starts_with("INSERT INTO ") {
            let (transformed_line, table_name) = transform_insert_for_all_tables(line, debug_mode)?;

            if transformed_line != line {
                transformed_count += 1;
            }

            if let Some(table) = table_name {
                *table_stats.entry(table).or_insert(0) += 1;
            }

            postgres_sql.push_str(&transformed_line);
            postgres_sql.push('\n');
        } else if !trimmed.is_empty() {
            postgres_sql.push_str(line);
            postgres_sql.push('\n');
        }

        if line_count % 10000 == 0 {
            timed_println(&format!("  Processed {} lines...", line_count));
        }
    }

    // Write output
    timed_println(&format!("💾 Writing PostgreSQL SQL: {}", output_file));
    fs::write(output_file, postgres_sql)?;

    // Print statistics
    timed_println("📊 Migration Statistics:");
    timed_println(&format!("  Total lines processed: {}", line_count));
    timed_println(&format!(
        "  INSERT statements transformed: {}",
        transformed_count
    ));

    for (table, count) in table_stats.iter() {
        timed_println(&format!("  {}: {} records", table, count));
    }

    timed_println("✅ Migration completed successfully!");
    timed_println(&format!("💡 Next steps:"));
    timed_println(&format!("  1. Review the output file: {}", output_file));
    timed_println(&format!(
        "  2. Import to PostgreSQL: psql $DATABASE_URL -f {}",
        output_file
    ));

    Ok(())
}

fn should_skip_line(line: &str) -> bool {
    line.starts_with("PRAGMA ")
        || line.starts_with("BEGIN TRANSACTION")
        || line.starts_with("COMMIT")
        || line.contains("sqlite_sequence")
        || line.starts_with("ANALYZE")
        || line.is_empty()
}

fn transform_insert_for_all_tables(
    line: &str,
    debug_mode: bool,
) -> Result<(String, Option<String>)> {
    let line = line.trim();

    // Handle articles table (18-column version)
    if line.contains("INSERT INTO articles VALUES(") {
        let transformed = transform_articles_insert(line)?;
        return Ok((transformed, Some("articles".to_string())));
    }

    // Handle entities table
    if line.contains("INSERT INTO entities VALUES(") {
        let transformed = transform_entities_insert(line)?;
        return Ok((transformed, Some("entities".to_string())));
    }

    // Handle article_entities table
    if line.contains("INSERT INTO article_entities VALUES(") {
        let transformed = transform_article_entities_insert(line)?;
        return Ok((transformed, Some("article_entities".to_string())));
    }

    // Handle configurations table
    if line.contains("INSERT INTO configurations VALUES(") {
        let transformed = transform_configurations_insert(line)?;
        return Ok((transformed, Some("configurations".to_string())));
    }

    // Handle entity_aliases table
    if line.contains("INSERT INTO entity_aliases VALUES(") {
        let transformed = transform_entity_aliases_insert(line)?;
        return Ok((transformed, Some("entity_aliases".to_string())));
    }

    // Handle article_clusters table
    if line.contains("INSERT INTO article_clusters VALUES(") {
        let transformed = transform_article_clusters_insert(line)?;
        return Ok((transformed, Some("article_clusters".to_string())));
    }

    // Handle article_cluster_mappings table
    if line.contains("INSERT INTO article_cluster_mappings VALUES(") {
        let transformed = transform_article_cluster_mappings_insert(line)?;
        return Ok((transformed, Some("article_cluster_mappings".to_string())));
    }

    // Handle cluster_merge_history table
    if line.contains("INSERT INTO cluster_merge_history VALUES(") {
        let transformed = transform_cluster_merge_history_insert(line)?;
        return Ok((transformed, Some("cluster_merge_history".to_string())));
    }

    // Handle entity_negative_matches table
    if line.contains("INSERT INTO entity_negative_matches VALUES(") {
        let transformed = transform_entity_negative_matches_insert(line)?;
        return Ok((transformed, Some("entity_negative_matches".to_string())));
    }

    // Handle alias_pattern_stats table
    if line.contains("INSERT INTO alias_pattern_stats VALUES(") {
        let transformed = transform_alias_pattern_stats_insert(line)?;
        return Ok((transformed, Some("alias_pattern_stats".to_string())));
    }

    // Handle alias_review_batches table
    if line.contains("INSERT INTO alias_review_batches VALUES(") {
        let transformed = transform_alias_review_batches_insert(line)?;
        return Ok((transformed, Some("alias_review_batches".to_string())));
    }

    // Handle alias_review_items table
    if line.contains("INSERT INTO alias_review_items VALUES(") {
        let transformed = transform_alias_review_items_insert(line)?;
        return Ok((transformed, Some("alias_review_items".to_string())));
    }

    // Handle alias_cache_stats table
    if line.contains("INSERT INTO alias_cache_stats VALUES(") {
        let transformed = transform_alias_cache_stats_insert(line)?;
        return Ok((transformed, Some("alias_cache_stats".to_string())));
    }

    // Handle queue tables
    if line.contains("INSERT INTO rss_queue VALUES(") {
        let transformed = transform_rss_queue_insert(line)?;
        return Ok((transformed, Some("rss_queue".to_string())));
    }

    if line.contains("INSERT INTO matched_topics_queue VALUES(") {
        let transformed = transform_matched_topics_queue_insert(line)?;
        return Ok((transformed, Some("matched_topics_queue".to_string())));
    }

    if line.contains("INSERT INTO life_safety_queue VALUES(") {
        let transformed = transform_life_safety_queue_insert(line)?;
        return Ok((transformed, Some("life_safety_queue".to_string())));
    }

    // Handle device tables
    if line.contains("INSERT INTO devices VALUES(") {
        let transformed = transform_devices_insert(line)?;
        return Ok((transformed, Some("devices".to_string())));
    }

    if line.contains("INSERT INTO device_subscriptions VALUES(") {
        let transformed = transform_device_subscriptions_insert(line)?;
        return Ok((transformed, Some("device_subscriptions".to_string())));
    }

    if line.contains("INSERT INTO ip_logs VALUES(") {
        let transformed = transform_ip_logs_insert(line)?;
        return Ok((transformed, Some("ip_logs".to_string())));
    }

    // Handle alert tables
    if line.contains("INSERT INTO endpoint_timeout_events VALUES(") {
        let transformed = transform_endpoint_timeout_events_insert(line)?;
        return Ok((transformed, Some("endpoint_timeout_events".to_string())));
    }

    if line.contains("INSERT INTO endpoint_alerts VALUES(") {
        let transformed = transform_endpoint_alerts_insert(line)?;
        return Ok((transformed, Some("endpoint_alerts".to_string())));
    }

    // Handle migration_metadata table
    if line.contains("INSERT INTO migration_metadata VALUES(") {
        let transformed = transform_migration_metadata_insert(line)?;
        return Ok((transformed, Some("migration_metadata".to_string())));
    }

    // Handle article_cluster_members table (if it exists)
    if line.contains("INSERT INTO article_cluster_members VALUES(") {
        let transformed = transform_article_cluster_members_insert(line)?;
        return Ok((transformed, Some("article_cluster_members".to_string())));
    }

    // Handle user_cluster_preferences table (if it exists)
    if line.contains("INSERT INTO user_cluster_preferences VALUES(") {
        let transformed = transform_user_cluster_preferences_insert(line)?;
        return Ok((transformed, Some("user_cluster_preferences".to_string())));
    }

    // Extract table name for unknown tables
    if line.starts_with("INSERT INTO ") {
        if let Some(table_name) = extract_table_name(line) {
            if debug_mode {
                println!(
                    "⚠️  Unhandled table: {} - passing through unchanged",
                    table_name
                );
            }
            return Ok((line.to_string(), Some(table_name)));
        }
    }

    // Pass through unchanged
    Ok((line.to_string(), None))
}

// Article transformation (7-column SQLite to PostgreSQL mapping)
fn transform_articles_insert(line: &str) -> Result<String> {
    // SQLite articles table has 7 columns: id, url, seen_at, is_relevant, category, analysis, r2_url
    // Map these to the corresponding PostgreSQL columns
    let replacement =
        "INSERT INTO articles (id, url, seen_at, is_relevant, category, analysis, r2_url) VALUES(";
    let result = line.replace("INSERT INTO articles VALUES(", replacement);
    Ok(transform_booleans(result))
}

// Entity transformations
fn transform_entities_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO entities (id, name, type, normalized_name, parent_id) VALUES(";
    let result = line.replace("INSERT INTO entities VALUES(", replacement);
    Ok(result)
}

fn transform_article_entities_insert(line: &str) -> Result<String> {
    let replacement =
        "INSERT INTO article_entities (id, article_id, entity_id, importance, context) VALUES(";
    let result = line.replace("INSERT INTO article_entities VALUES(", replacement);
    Ok(result)
}

// Configuration transformation
fn transform_configurations_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO configurations (id, category, name, value, enabled, created_at, updated_at) VALUES(";
    let result = line.replace("INSERT INTO configurations VALUES(", replacement);
    Ok(transform_booleans(result))
}

// Alias system transformations
fn transform_entity_aliases_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO entity_aliases (id, entity_id, canonical_name, alias_text, normalized_canonical, normalized_alias, entity_type, source, confidence, created_at, approved_by, approved_at, status) VALUES(";
    let result = line.replace("INSERT INTO entity_aliases VALUES(", replacement);
    Ok(result)
}

fn transform_entity_negative_matches_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO entity_negative_matches (id, entity_id1, entity_id2, normalized_name1, normalized_name2, entity_type, rejected_by, rejected_at, rejection_reason, persistence_level) VALUES(";
    let result = line.replace("INSERT INTO entity_negative_matches VALUES(", replacement);
    Ok(result)
}

fn transform_alias_pattern_stats_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO alias_pattern_stats (pattern_id, pattern_type, total_suggestions, approved_count, rejected_count, last_used_at, enabled) VALUES(";
    let result = line.replace("INSERT INTO alias_pattern_stats VALUES(", replacement);
    Ok(transform_booleans(result))
}

fn transform_alias_review_batches_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO alias_review_batches (id, created_at, admin_id, status, total_count, processed_count) VALUES(";
    let result = line.replace("INSERT INTO alias_review_batches VALUES(", replacement);
    Ok(result)
}

fn transform_alias_review_items_insert(line: &str) -> Result<String> {
    let replacement =
        "INSERT INTO alias_review_items (id, batch_id, alias_id, decision, decided_at) VALUES(";
    let result = line.replace("INSERT INTO alias_review_items VALUES(", replacement);
    Ok(result)
}

fn transform_alias_cache_stats_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO alias_cache_stats (normalized_name, entity_type, hit_count, last_accessed) VALUES(";
    let result = line.replace("INSERT INTO alias_cache_stats VALUES(", replacement);
    Ok(result)
}

// Cluster transformations
fn transform_article_clusters_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO article_clusters (id, name, creation_date, last_updated, primary_entity_ids, article_count, needs_summary_update, summary, summary_version, status, importance_score, has_timeline) VALUES(";
    let result = line.replace("INSERT INTO article_clusters VALUES(", replacement);
    Ok(result)
}

fn transform_article_cluster_mappings_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO article_cluster_mappings (id, article_id, cluster_id, added_date, similarity_score) VALUES(";
    let result = line.replace("INSERT INTO article_cluster_mappings VALUES(", replacement);
    Ok(result)
}

fn transform_cluster_merge_history_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO cluster_merge_history (id, original_cluster_id, merged_into_cluster_id, merge_date, merge_reason) VALUES(";
    let result = line.replace("INSERT INTO cluster_merge_history VALUES(", replacement);
    Ok(result)
}

// Queue transformations
fn transform_rss_queue_insert(line: &str) -> Result<String> {
    // SQLite rss_queue has only 2 columns: id, url
    // Map to corresponding PostgreSQL columns
    let replacement = "INSERT INTO rss_queue (id, url) VALUES(";
    let result = line.replace("INSERT INTO rss_queue VALUES(", replacement);
    Ok(result)
}

fn transform_matched_topics_queue_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO matched_topics_queue (id, article_text, article_html, article_url, article_title, topic_matched, article_hash, title_domain_hash, timestamp, pub_date) VALUES(";
    let result = line.replace("INSERT INTO matched_topics_queue VALUES(", replacement);
    Ok(result)
}

fn transform_life_safety_queue_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO life_safety_queue (id, article_url, article_title, article_text, article_html, article_hash, title_domain_hash, threat, timestamp, pub_date) VALUES(";
    let result = line.replace("INSERT INTO life_safety_queue VALUES(", replacement);
    Ok(result)
}

// Device transformations
fn transform_devices_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO devices (id, device_id) VALUES(";
    let result = line.replace("INSERT INTO devices VALUES(", replacement);
    Ok(result)
}

fn transform_device_subscriptions_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO device_subscriptions (id, device_id, topic, priority) VALUES(";
    let result = line.replace("INSERT INTO device_subscriptions VALUES(", replacement);
    Ok(result)
}

fn transform_ip_logs_insert(line: &str) -> Result<String> {
    let replacement =
        "INSERT INTO ip_logs (id, device_id, ip_address, first_seen, last_seen) VALUES(";
    let result = line.replace("INSERT INTO ip_logs VALUES(", replacement);
    Ok(result)
}

// Alert transformations
fn transform_endpoint_timeout_events_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO endpoint_timeout_events (id, endpoint_url, model_name, worker_id, worker_type, timeout_type, occurred_at) VALUES(";
    let result = line.replace("INSERT INTO endpoint_timeout_events VALUES(", replacement);
    Ok(result)
}

fn transform_endpoint_alerts_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO endpoint_alerts (id, endpoint_url, model_name, alert_type, first_occurrence, last_occurrence, last_alert_sent, occurrence_count, consecutive_failures, is_resolved, resolved_at) VALUES(";
    let result = line.replace("INSERT INTO endpoint_alerts VALUES(", replacement);
    Ok(transform_booleans(result))
}

// Migration metadata transformation
fn transform_migration_metadata_insert(line: &str) -> Result<String> {
    let replacement = "INSERT INTO migration_metadata (key, value, created_at) VALUES(";
    let result = line.replace("INSERT INTO migration_metadata VALUES(", replacement);
    Ok(result)
}

// Additional table transformations for tables found in your database
fn transform_article_cluster_members_insert(line: &str) -> Result<String> {
    // Pass through unchanged - this table might not exist in PostgreSQL schema
    // or needs to be mapped to article_cluster_mappings
    Ok(line.to_string())
}

fn transform_user_cluster_preferences_insert(line: &str) -> Result<String> {
    // Pass through unchanged - this table might not exist in PostgreSQL schema
    Ok(line.to_string())
}

fn transform_booleans(mut text: String) -> String {
    // Transform SQLite boolean values to PostgreSQL
    text = text.replace(",0,", ",false,");
    text = text.replace(",1,", ",true,");
    text = text.replace(",0)", ",false)");
    text = text.replace(",1)", ",true)");
    text = text.replace("(0,", "(false,");
    text = text.replace("(1,", "(true,");

    text
}

fn extract_table_name(line: &str) -> Option<String> {
    if let Some(start) = line.find("INSERT INTO ") {
        let after_insert = &line[start + 12..];
        if let Some(end) = after_insert.find(" VALUES(") {
            return Some(after_insert[..end].trim().to_string());
        }
    }
    None
}
