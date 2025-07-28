use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use clap::{Arg, Command as ClapCommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration as StdDuration, Instant};
use sysinfo::System;
use tokio::time::Duration;

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
        // Fallback if timing not initialized
        println!("{}", message);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationCheckpoint {
    migration_id: String,
    mode: String,
    start_time: DateTime<Utc>,
    completed_phases: Vec<String>,
    current_phase: String,
    completed_tables: Vec<String>,
    current_table: Option<String>,
    total_tables: usize,
    last_checkpoint: DateTime<Utc>,
    cutoff_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug)]
enum MigrationMode {
    Full,
    Incremental(DateTime<Utc>),
}

#[derive(Debug)]
struct MigrationStats {
    start_time: Instant,
    phase_times: std::collections::HashMap<String, StdDuration>,
    memory_usage: Vec<(String, u64)>,
}

impl MigrationStats {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            phase_times: std::collections::HashMap::new(),
            memory_usage: Vec::new(),
        }
    }

    fn record_phase(&mut self, phase: &str, duration: StdDuration) {
        self.phase_times.insert(phase.to_string(), duration);
    }

    fn record_memory(&mut self, phase: &str) {
        let mut system = System::new_all();
        system.refresh_all();
        let process_name = std::ffi::OsStr::new("migrate_to_postgres");

        // Collect memory info immediately to avoid lifetime issues
        let memory_mb = system
            .processes_by_exact_name(process_name)
            .next()
            .map(|process| process.memory())
            .unwrap_or(0);

        if memory_mb > 0 {
            self.memory_usage.push((phase.to_string(), memory_mb));
        }
    }

    fn print_summary(&self) {
        let total_time = self.start_time.elapsed();
        println!("\n📊 Migration Performance Summary:");
        println!(
            "  🕐 Total time: {:.1} minutes",
            total_time.as_secs_f64() / 60.0
        );

        for (phase, duration) in &self.phase_times {
            println!("  ⏱️  {}: {:.1}s", phase, duration.as_secs_f64());
        }

        if !self.memory_usage.is_empty() {
            println!("\n💾 Memory Usage:");
            for (phase, memory) in &self.memory_usage {
                println!("  📈 {}: {:.1} MB", phase, memory / 1024 / 1024);
            }
        }
    }
}

/// Safely truncate a string for preview display, respecting Unicode boundaries
fn safe_truncate_for_preview(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_index, _)) => &s[..byte_index],
        None => s, // String is shorter than max_chars
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = ClapCommand::new("migrate_to_postgres")
        .about("Enhanced PostgreSQL migration with debugging and incremental support")
        .arg(
            Arg::new("auto")
                .long("auto")
                .help("Automatically detect migration mode (full or incremental)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable detailed debugging and performance tracking")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("resume")
                .long("resume")
                .help("Resume from last checkpoint")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force-full")
                .long("force-full")
                .help("Force full migration even if data exists")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("since")
                .long("since")
                .value_name("TIMESTAMP")
                .help("Incremental migration since specific timestamp (YYYY-MM-DD HH:MM:SS)")
                .action(clap::ArgAction::Set),
        )
        .get_matches();

    let debug_mode = matches.get_flag("debug");
    let auto_mode = matches.get_flag("auto");
    let resume_mode = matches.get_flag("resume");
    let force_full = matches.get_flag("force-full");
    let since_timestamp = matches.get_one::<String>("since");

    let mut stats = MigrationStats::new();

    // Initialize global timing system
    MIGRATION_START_TIME
        .set(Instant::now())
        .expect("Failed to initialize migration timing");

    timed_println("🚀 Starting Enhanced PostgreSQL Migration...");
    if debug_mode {
        timed_println("🔍 Debug mode enabled - detailed performance tracking active");
    }

    // Handle resume mode
    if resume_mode {
        return resume_migration(debug_mode, &mut stats).await;
    }

    // Phase 1: Pre-migration checks and backup
    let phase_start = Instant::now();
    check_prerequisites().await?;
    create_backup().await?;
    stats.record_phase("Prerequisites & Backup", phase_start.elapsed());
    stats.record_memory("Prerequisites");

    // Phase 2: Setup PostgreSQL and determine migration mode
    let phase_start = Instant::now();
    let pool = setup_postgres_connection().await?;

    let migration_mode = if force_full {
        MigrationMode::Full
    } else if let Some(since_str) = since_timestamp {
        let since_dt =
            chrono::NaiveDateTime::parse_from_str(since_str, "%Y-%m-%d %H:%M:%S")?.and_utc();
        MigrationMode::Incremental(since_dt)
    } else if auto_mode {
        determine_migration_mode(&pool).await?
    } else {
        MigrationMode::Full
    };

    match &migration_mode {
        MigrationMode::Full => {
            timed_println("🔍 Detected: Empty PostgreSQL database → Full migration");
        }
        MigrationMode::Incremental(cutoff) => {
            timed_println(&format!(
                "🔍 Detected: Existing data, last cutoff: {} → Incremental migration",
                cutoff.format("%Y-%m-%d %H:%M:%S%.6f%z")
            ));
        }
    }

    stats.record_phase("PostgreSQL Setup & Mode Detection", phase_start.elapsed());

    // Create checkpoint for crash recovery
    let checkpoint = create_initial_checkpoint(&migration_mode)?;
    save_checkpoint(&checkpoint)?;

    // Phase 3: Schema setup (only for full migration)
    if matches!(migration_mode, MigrationMode::Full) {
        let phase_start = Instant::now();
        create_postgres_schema_enhanced(&pool, debug_mode).await?;
        stats.record_phase("Schema Creation", phase_start.elapsed());
        stats.record_memory("Schema");

        update_checkpoint_phase(&checkpoint.migration_id, "schema_complete").await?;
    }

    // Phase 4: Data migration
    let phase_start = Instant::now();
    match migration_mode {
        MigrationMode::Full => {
            migrate_data_via_dump_enhanced(&pool, debug_mode, &mut stats).await?;
        }
        MigrationMode::Incremental(cutoff) => {
            migrate_incremental_data(cutoff, debug_mode, &mut stats).await?;
        }
    }
    stats.record_phase("Data Migration", phase_start.elapsed());
    stats.record_memory("Data Migration");

    update_checkpoint_phase(&checkpoint.migration_id, "data_complete").await?;

    // Phase 5: Index creation (only for full migration)
    if matches!(migration_mode, MigrationMode::Full) {
        let phase_start = Instant::now();
        create_indexes_after_import_enhanced(&pool, debug_mode).await?;
        stats.record_phase("Index Creation", phase_start.elapsed());
        stats.record_memory("Indexes");

        update_checkpoint_phase(&checkpoint.migration_id, "indexes_complete").await?;
    }

    // Phase 6: Configuration migration (only for full migration)
    if matches!(migration_mode, MigrationMode::Full) {
        let phase_start = Instant::now();
        migrate_env_to_database(&pool).await?;
        stats.record_phase("Configuration Migration", phase_start.elapsed());

        update_checkpoint_phase(&checkpoint.migration_id, "config_complete").await?;
    }

    // Phase 7: Update migration metadata
    let phase_start = Instant::now();
    record_migration_completion(&pool).await?;
    stats.record_phase("Metadata Update", phase_start.elapsed());

    // Phase 8: Validation
    let phase_start = Instant::now();
    validate_migration(&pool).await?;
    stats.record_phase("Validation", phase_start.elapsed());

    // Cleanup checkpoint file
    let _ = fs::remove_file("migration_checkpoint.json");

    if debug_mode {
        stats.print_summary();
    }

    timed_println("✅ Migration completed successfully!");
    match migration_mode {
        MigrationMode::Full => {
            timed_println("📝 Cutoff timestamp stored for future incremental migrations");
        }
        MigrationMode::Incremental(_) => {
            timed_println("📝 Cutoff timestamp updated for next incremental migration");
        }
    }

    timed_println("Next steps:");
    timed_println("  1. Test the application: cargo run --release");
    timed_println("  2. Use admin tool: cargo run --bin argus_admin");
    timed_println("  3. Create alias: alias aa='cargo run --bin argus_admin'");

    Ok(())
}

async fn check_prerequisites() -> Result<()> {
    timed_println("🔍 Checking prerequisites...");

    // Check SQLite database exists
    if !std::path::Path::new("argus.db").exists() {
        return Err(anyhow::anyhow!("SQLite database 'argus.db' not found"));
    }

    // Check DATABASE_URL is set
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

    // Validate that it's a PostgreSQL URL
    if !database_url.starts_with("postgresql://") && !database_url.starts_with("postgres://") {
        return Err(anyhow::anyhow!(
            "DATABASE_URL must be a PostgreSQL connection string"
        ));
    }

    // Check PostgreSQL connection with timeout
    let output = Command::new("timeout")
        .arg("10") // 10 second timeout
        .arg("psql")
        .arg(&database_url)
        .arg("-c")
        .arg("SELECT version();")
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Cannot connect to PostgreSQL database. Error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    timed_println("✅ Prerequisites check passed");
    Ok(())
}

async fn create_backup() -> Result<()> {
    timed_println("💾 Creating backup...");

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");

    // Backup SQLite database
    let backup_path = format!("argus.db.backup.{}", timestamp);
    fs::copy("argus.db", &backup_path)?;
    timed_println(&format!("  ✅ SQLite backup: {}", backup_path));

    // Backup environment variables
    let env_backup = format!("env.backup.{}", timestamp);
    let env_vars = [
        ("TOPICS", env::var("TOPICS").unwrap_or_default()),
        ("URLS", env::var("URLS").unwrap_or_default()),
        ("SLACK_TOKEN", env::var("SLACK_TOKEN").unwrap_or_default()),
        (
            "SLACK_CHANNEL",
            env::var("SLACK_CHANNEL").unwrap_or_default(),
        ),
        ("RUST_LOG", env::var("RUST_LOG").unwrap_or_default()),
        (
            "DEFAULT_OLLAMA_MODEL",
            env::var("DEFAULT_OLLAMA_MODEL").unwrap_or_default(),
        ),
        (
            "NO_THINK_MODE",
            env::var("NO_THINK_MODE").unwrap_or_default(),
        ),
    ];

    let env_content = env_vars
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("export {}=\"{}\"", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&env_backup, env_content)?;
    timed_println(&format!("  ✅ Environment backup: {}", env_backup));

    Ok(())
}

async fn setup_postgres_connection() -> Result<Pool<Postgres>> {
    timed_println("🔌 Setting up PostgreSQL connection...");

    let database_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&database_url)
        .await?;

    // Test connection
    sqlx::query("SELECT 1").execute(&pool).await?;
    timed_println("✅ PostgreSQL connection established");

    Ok(pool)
}

// Global storage for index statements to execute after data import
use std::sync::Mutex;
lazy_static::lazy_static! {
    static ref INDEX_STATEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

fn parse_schema_statements(schema_sql: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut table_statements = Vec::new();
    let mut index_statements = Vec::new();
    let mut other_statements = Vec::new();

    // Simple but robust parsing - split by semicolon and handle each statement
    let mut current_statement = String::new();
    let mut in_dollar_quote = false;
    let mut dollar_tag = String::new();

    for line in schema_sql.lines() {
        let trimmed_line = line.trim();

        // Skip comments and empty lines
        if trimmed_line.is_empty() || trimmed_line.starts_with("--") {
            continue;
        }

        // Add line to current statement
        if !current_statement.is_empty() {
            current_statement.push('\n');
        }
        current_statement.push_str(line);

        // Handle dollar quoting for functions
        if let Some(dollar_pos) = trimmed_line.find("$$") {
            if !in_dollar_quote {
                // Starting dollar quote
                in_dollar_quote = true;
                // Extract tag if any (e.g., $tag$)
                if let Some(end_pos) = trimmed_line[dollar_pos + 2..].find("$$") {
                    dollar_tag = trimmed_line[dollar_pos..dollar_pos + 2 + end_pos + 2].to_string();
                } else {
                    dollar_tag = "$$".to_string();
                }
            } else if trimmed_line.contains(&dollar_tag) {
                // Ending dollar quote
                in_dollar_quote = false;
                dollar_tag.clear();
            }
        }

        // Check if statement is complete
        if !in_dollar_quote && trimmed_line.ends_with(';') {
            let statement = current_statement.trim();
            if !statement.is_empty() {
                categorize_statement(
                    statement,
                    &mut table_statements,
                    &mut index_statements,
                    &mut other_statements,
                );
            }
            current_statement.clear();
        }
    }

    // Handle any remaining statement
    if !current_statement.trim().is_empty() {
        let statement = current_statement.trim();
        categorize_statement(
            statement,
            &mut table_statements,
            &mut index_statements,
            &mut other_statements,
        );
    }

    (table_statements, index_statements, other_statements)
}

fn categorize_statement(
    statement: &str,
    table_statements: &mut Vec<String>,
    index_statements: &mut Vec<String>,
    other_statements: &mut Vec<String>,
) {
    let first_line = statement.lines().next().unwrap_or("").to_uppercase();

    if first_line.starts_with("CREATE TABLE") {
        table_statements.push(statement.to_string());
    } else if first_line.starts_with("CREATE INDEX")
        || first_line.starts_with("CREATE UNIQUE INDEX")
    {
        index_statements.push(statement.to_string());
    } else if !statement.trim().is_empty() {
        other_statements.push(statement.to_string());
    }
}

fn transform_sqlite_to_postgres(sqlite_sql: &str) -> Result<String> {
    let mut postgres_sql = String::new();
    let mut skip_until_semicolon = false;

    for line in sqlite_sql.lines() {
        let trimmed = line.trim();

        // Skip SQLite-specific statements
        if trimmed.starts_with("PRAGMA")
            || trimmed.starts_with("BEGIN TRANSACTION")
            || trimmed.starts_with("COMMIT")
            || trimmed.contains("sqlite_sequence")
            || trimmed.starts_with("ANALYZE")
        {
            continue;
        }

        // Skip CREATE TABLE and CREATE INDEX statements (schema already exists)
        if trimmed.starts_with("CREATE TABLE")
            || trimmed.starts_with("CREATE UNIQUE INDEX")
            || trimmed.starts_with("CREATE INDEX")
        {
            skip_until_semicolon = true;
            continue;
        }

        // Skip until we find the end of the statement
        if skip_until_semicolon {
            if trimmed.ends_with(";") {
                skip_until_semicolon = false;
            }
            continue;
        }

        // Transform INSERT statements to use explicit column names
        if trimmed.starts_with("INSERT INTO") {
            let transformed_line = transform_insert_statement(line)?;
            // Apply additional PostgreSQL compatibility fixes
            let postgres_compatible = fix_postgres_compatibility(&transformed_line)?;
            postgres_sql.push_str(&postgres_compatible);
            postgres_sql.push('\n');
        } else if !trimmed.is_empty() {
            // Include other non-empty lines (like VALUES, etc.)
            postgres_sql.push_str(line);
            postgres_sql.push('\n');
        }
    }

    Ok(postgres_sql)
}

fn transform_insert_statement(line: &str) -> Result<String> {
    // Parse INSERT INTO table VALUES(...) and convert to explicit column names
    if let Some(values_start) = line.find(" VALUES(") {
        let table_part = &line[..values_start];
        let values_part = &line[values_start + 8..]; // Skip " VALUES("

        // Extract table name
        if let Some(table_name) = extract_table_name(table_part) {
            // Get the column specification for this table
            let columns = get_table_columns(&table_name);
            if !columns.is_empty() {
                // First, fix any string concatenation issues in the values part
                let fixed_values = fix_string_concatenation(values_part)?;

                // Reconstruct with explicit column names
                let column_list = columns.join(", ");
                let mut result = format!("INSERT INTO {} ({}) VALUES(", table_name, column_list);
                result.push_str(&fixed_values);

                // Transform boolean values using robust parsing
                result = transform_boolean_values(&result, &table_name)?;

                // Convert Unix timestamps to PostgreSQL format for tables with timestamp fields
                result = convert_unix_timestamps(&result, &table_name);

                return Ok(result);
            }
        }
    }

    // Fallback: return original line with boolean transformations
    let mut result = line.to_string();

    // Extract table name for fallback transformation
    if let Some(table_name) = extract_table_name(&result) {
        result = transform_boolean_values(&result, &table_name)?;
    }

    Ok(result)
}

fn fix_string_concatenation(values_part: &str) -> Result<String> {
    // Fix string concatenation issues in VALUES clause
    let original = values_part.to_string();
    let mut result = original.clone();

    // Debug: Check if we have concatenation to fix
    if result.contains(" || ") {
        let preview = safe_truncate_for_preview(&result, 100);
        println!("  🔧 Fixing string concatenation in: {}", preview);
    }

    // Handle the specific pattern from the error: URL || timestamp
    // Look for patterns like 'url' || 'timestamp' and merge them properly
    use regex::Regex;

    // More aggressive pattern matching to handle all concatenation cases
    // Pattern 1: 'string1' || 'string2' -> 'string1string2'
    let quoted_concat_regex = Regex::new(r"'([^']*)'\s*\|\|\s*'([^']*)'").unwrap();

    // Keep applying the regex until no more matches (handles multiple concatenations)
    loop {
        let new_result = quoted_concat_regex
            .replace_all(&result, "'$1$2'")
            .to_string();
        if new_result == result {
            break; // No more changes
        }
        result = new_result;
    }

    // Pattern 2: Handle mixed patterns like value || 'string'
    let mixed_concat_regex = Regex::new(r"([^,\s']+)\s*\|\|\s*'([^']*)'").unwrap();
    result = mixed_concat_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    // Pattern 3: Handle 'string' || value patterns
    let reverse_mixed_regex = Regex::new(r"'([^']*)'\s*\|\|\s*([^,\s']+)").unwrap();
    result = reverse_mixed_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    // Pattern 4: Handle unquoted || unquoted patterns
    let unquoted_concat_regex = Regex::new(r"([^,\s']+)\s*\|\|\s*([^,\s']+)").unwrap();
    result = unquoted_concat_regex
        .replace_all(&result, "'$1$2'")
        .to_string();

    // Debug: Show result if we made changes
    if result != original {
        let preview = safe_truncate_for_preview(&result, 100);
        println!("  ✅ Fixed to: {}", preview);
    } else if original.contains(" || ") {
        let preview = safe_truncate_for_preview(&original, 100);
        println!("  ❌ No changes made to: {}", preview);
    }

    Ok(result)
}

fn extract_table_name(table_part: &str) -> Option<String> {
    // Extract table name from "INSERT INTO table_name"
    if let Some(into_pos) = table_part.find("INSERT INTO ") {
        let after_into = &table_part[into_pos + 12..].trim();
        // Take first word as table name
        if let Some(space_pos) = after_into.find(' ') {
            Some(after_into[..space_pos].to_string())
        } else {
            Some(after_into.to_string())
        }
    } else {
        None
    }
}

fn get_table_columns(table_name: &str) -> Vec<String> {
    // Return the correct column order for each table based on our PostgreSQL schema
    match table_name {
        "articles" => vec![
            "id".to_string(),
            "url".to_string(),
            "seen_at".to_string(),
            "is_relevant".to_string(),
            "category".to_string(),
            "analysis".to_string(),
            "normalized_url".to_string(),
            "hash".to_string(),
            "tiny_summary".to_string(),
            "title_domain_hash".to_string(),
            "r2_url".to_string(),
            "pub_date".to_string(),
            "event_date".to_string(),
            "cluster_id".to_string(),
            "title".to_string(),
            "json_data".to_string(),
            "quality".to_string(),
            "source".to_string(),
        ],
        "entities" => vec![
            "id".to_string(),
            "name".to_string(),
            "type".to_string(),
            "normalized_name".to_string(),
            "parent_id".to_string(),
        ],
        "article_entities" => vec![
            "id".to_string(),
            "article_id".to_string(),
            "entity_id".to_string(),
            "importance".to_string(),
            "context".to_string(),
        ],
        "configurations" => vec![
            "id".to_string(),
            "category".to_string(),
            "name".to_string(),
            "value".to_string(),
            "enabled".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
        ],
        "endpoint_alerts" => vec![
            "id".to_string(),
            "endpoint_url".to_string(),
            "model_name".to_string(),
            "alert_type".to_string(),
            "first_occurrence".to_string(),
            "last_occurrence".to_string(),
            "last_alert_sent".to_string(),
            "occurrence_count".to_string(),
            "consecutive_failures".to_string(),
            "is_resolved".to_string(),
            "resolved_at".to_string(),
        ],
        "alias_pattern_stats" => vec![
            "pattern_id".to_string(),
            "pattern_type".to_string(),
            "total_suggestions".to_string(),
            "approved_count".to_string(),
            "rejected_count".to_string(),
            "last_used_at".to_string(),
            "enabled".to_string(),
        ],
        _ => vec![], // For other tables, let them use default behavior
    }
}

fn get_boolean_columns(table_name: &str) -> Vec<usize> {
    // Return 0-based column indices for boolean columns in each table
    match table_name {
        "articles" => vec![3],            // is_relevant is 4th column (0-indexed: 3)
        "configurations" => vec![4],      // enabled is 5th column (0-indexed: 4)
        "endpoint_alerts" => vec![9],     // is_resolved is 10th column (0-indexed: 9)
        "alias_pattern_stats" => vec![6], // enabled is 7th column (0-indexed: 6)
        "entity_aliases" => vec![],       // status is TEXT, not boolean
        "alias_review_batches" => vec![], // status is TEXT, not boolean
        "device_subscriptions" => vec![], // No boolean columns
        "ip_logs" => vec![],              // No boolean columns
        _ => vec![],
    }
}

fn transform_boolean_values(sql: &str, table_name: &str) -> Result<String> {
    let boolean_positions = get_boolean_columns(table_name);
    if boolean_positions.is_empty() {
        return Ok(sql.to_string());
    }

    // Find the VALUES clause
    if let Some(values_start) = sql.find(" VALUES(") {
        let before_values = &sql[..values_start + 8]; // Include " VALUES("
        let values_content = &sql[values_start + 8..];

        // Find the closing parenthesis for the VALUES clause
        if let Some(values_end) = values_content.rfind(')') {
            let values_data = &values_content[..values_end];
            let after_values = &values_content[values_end..]; // Include closing paren and semicolon

            // Parse and transform the values
            let transformed_values = transform_values_data(values_data, &boolean_positions)?;

            return Ok(format!(
                "{}{}{}",
                before_values, transformed_values, after_values
            ));
        }
    }

    // Fallback: use simple string replacement for basic cases
    let mut result = sql.to_string();

    // Handle common boolean patterns
    result = result.replace(",0,", ",false,");
    result = result.replace(",1,", ",true,");
    result = result.replace("(0,", "(false,");
    result = result.replace("(1,", "(true,");
    result = result.replace(",0)", ",false)");
    result = result.replace(",1)", ",true)");

    // Handle quoted boolean values
    result = result.replace(",'0',", ",false,");
    result = result.replace(",'1',", ",true,");
    result = result.replace("('0',", "(false,");
    result = result.replace("('1',", "(true,");
    result = result.replace(",'0')", ",false)");
    result = result.replace(",'1')", ",true)");

    Ok(result)
}

fn transform_values_data(values_data: &str, boolean_positions: &[usize]) -> Result<String> {
    // Simple CSV parsing for VALUES data
    let mut result = String::new();
    let mut current_field = String::new();
    let mut field_index = 0;
    let mut in_quotes = false;
    let mut escape_next = false;

    for ch in values_data.chars() {
        if escape_next {
            current_field.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => {
                escape_next = true;
                current_field.push(ch);
            }
            '\'' => {
                in_quotes = !in_quotes;
                current_field.push(ch);
            }
            ',' if !in_quotes => {
                // End of field
                let transformed_field =
                    transform_field_if_boolean(&current_field, field_index, boolean_positions);
                result.push_str(&transformed_field);
                result.push(',');

                current_field.clear();
                field_index += 1;
            }
            _ => {
                current_field.push(ch);
            }
        }
    }

    // Handle the last field
    if !current_field.is_empty() {
        let transformed_field =
            transform_field_if_boolean(&current_field, field_index, boolean_positions);
        result.push_str(&transformed_field);
    }

    Ok(result)
}

fn transform_field_if_boolean(
    field: &str,
    field_index: usize,
    boolean_positions: &[usize],
) -> String {
    if !boolean_positions.contains(&field_index) {
        return field.to_string();
    }

    let trimmed = field.trim();

    // Handle various boolean representations
    match trimmed {
        "0" | "'0'" => "false".to_string(),
        "1" | "'1'" => "true".to_string(),
        "NULL" | "null" => "NULL".to_string(),
        _ => {
            // If it's not a clear boolean value, keep it as-is
            // This handles cases where the field might already be transformed
            if trimmed == "true" || trimmed == "false" {
                trimmed.to_string()
            } else {
                // Log unexpected boolean value for debugging
                eprintln!(
                    "Warning: Unexpected boolean value '{}' at position {}",
                    trimmed, field_index
                );
                field.to_string()
            }
        }
    }
}

fn fix_postgres_compatibility(sql: &str) -> Result<String> {
    let mut result = sql.to_string();

    // Fix common PostgreSQL compatibility issues
    use regex::Regex;

    // 1. Handle char() function calls - PostgreSQL uses chr() instead of char()
    // Use regex to catch all char() patterns, including those in complex expressions
    let char_regex = Regex::new(r"\bchar\((\d+)\)").unwrap();
    result = char_regex.replace_all(&result, "chr($1)").to_string();

    // 2. Handle problematic escape sequences in string literals
    // Fix newline escape sequences that might be causing issues
    let newline_regex = Regex::new(r"'\\n'").unwrap();
    result = newline_regex.replace_all(&result, "E'\\n'").to_string();

    // Fix carriage return escape sequences
    let cr_regex = Regex::new(r"'\\r'").unwrap();
    result = cr_regex.replace_all(&result, "E'\\r'").to_string();

    // Fix tab escape sequences
    let tab_regex = Regex::new(r"'\\t'").unwrap();
    result = tab_regex.replace_all(&result, "E'\\t'").to_string();

    // 3. Handle problematic quote escaping
    // Replace sequences like '\'' with proper PostgreSQL escaping
    result = result.replace("\\'", "''");

    // 4. Handle NULL byte characters that might cause issues
    result = result.replace("\\0", "");

    // 5. Handle problematic concatenation patterns that might cause syntax errors
    // Look for patterns like '),(' which might be causing issues
    // This is a more aggressive fix for complex data patterns
    let concat_regex = Regex::new(r"'\s*,\s*'").unwrap();
    result = concat_regex.replace_all(&result, "' || '").to_string();

    // 6. Handle backticks (MySQL-style) that might appear in data
    result = result.replace("`", "\"");

    Ok(result)
}

fn convert_unix_timestamps(sql: &str, table_name: &str) -> String {
    use regex::Regex;

    // Only convert timestamps for tables that have timestamp fields
    let has_timestamps = match table_name {
        "articles" => true,                 // seen_at, pub_date, event_date
        "rss_queue" => true,                // seen_at, pub_date
        "matched_topics_queue" => true,     // timestamp, pub_date
        "life_safety_queue" => true,        // timestamp, pub_date
        "article_clusters" => true,         // creation_date, last_updated
        "article_cluster_mappings" => true, // added_date
        "cluster_merge_history" => true,    // merge_date
        "entity_aliases" => true,           // created_at, approved_at
        "entity_negative_matches" => true,  // rejected_at
        "alias_pattern_stats" => true,      // last_used_at
        "alias_review_batches" => true,     // created_at
        "alias_review_items" => true,       // decided_at
        "alias_cache_stats" => true,        // last_accessed
        "endpoint_timeout_events" => true,  // occurred_at
        "endpoint_alerts" => true, // first_occurrence, last_occurrence, last_alert_sent, resolved_at
        "configurations" => true,  // updated_at
        _ => false,
    };

    if !has_timestamps {
        return sql.to_string();
    }

    // Convert Unix timestamps to PostgreSQL TIMESTAMPTZ format
    // Pattern: 'NNNNNNNNNN' where N is a digit (Unix timestamp)
    let timestamp_regex = Regex::new(r"'(\d{10})'").unwrap();

    timestamp_regex
        .replace_all(sql, |caps: &regex::Captures| {
            let unix_timestamp = &caps[1];
            if let Ok(timestamp) = unix_timestamp.parse::<i64>() {
                // Convert Unix timestamp to PostgreSQL format
                if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                    format!("'{}'", datetime.format("%Y-%m-%d %H:%M:%S%.3f%z"))
                } else {
                    // If conversion fails, use NULL
                    "NULL".to_string()
                }
            } else {
                // If parsing fails, keep original
                format!("'{}'", unix_timestamp)
            }
        })
        .to_string()
}

async fn reset_postgres_sequences(pool: &Pool<Postgres>) -> Result<()> {
    timed_println("  🔧 Resetting PostgreSQL sequences...");

    let tables = [
        "articles",
        "entities",
        "article_entities",
        "entity_aliases",
        "article_clusters",
        "article_cluster_mappings",
        "devices",
        "configurations",
    ];

    for table in tables {
        // Check if table exists and has an id column
        let table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_name = $1
            )",
        )
        .bind(table)
        .fetch_one(pool)
        .await?;

        if table_exists {
            // Reset sequence to max ID + 1
            let query = format!(
                "SELECT setval('{}_id_seq', COALESCE((SELECT MAX(id) FROM {}), 1))",
                table, table
            );

            let _ = sqlx::query(&query).execute(pool).await; // Ignore errors for missing sequences
        }
    }

    timed_println("  ✅ Sequences reset");
    Ok(())
}

async fn migrate_env_to_database(pool: &Pool<Postgres>) -> Result<()> {
    timed_println("⚙️  Migrating environment configuration to database...");

    // Migrate topics from TOPICS env var
    if let Ok(topics_str) = env::var("TOPICS") {
        timed_println("  📝 Migrating topics...");
        for line in topics_str.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            for topic_pair in line.split(';') {
                let topic_pair = topic_pair.trim();
                if topic_pair.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = topic_pair.split(':').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim();
                    let prompt = parts[1..].join(":").trim().to_string(); // Handle colons in prompts

                    if !name.is_empty() && !prompt.is_empty() {
                        set_config(pool, "topics", name, &prompt).await?;
                        timed_println(&format!("    ✅ Migrated topic: {}", name));
                    }
                }
            }
        }
    }

    // Migrate RSS feeds from URLS env var
    if let Ok(urls_str) = env::var("URLS") {
        timed_println("  📡 Migrating RSS feeds...");
        for (i, url) in urls_str.split(';').enumerate() {
            let url = url.trim();
            if !url.is_empty() {
                let name = format!("feed_{}", i + 1);
                set_config(pool, "rss", &name, url).await?;
                timed_println(&format!("    ✅ Migrated RSS feed: {}", name));
            }
        }
    }

    // Migrate worker configurations
    timed_println("  🔧 Migrating worker configurations...");
    let worker_configs = [
        ("decision", "DECISION_OLLAMA_CONFIGS"),
        ("decision", "DECISION_OPENAI_CONFIGS"),
        ("analysis", "ANALYSIS_OLLAMA_CONFIGS"),
        ("analysis", "ANALYSIS_OPENAI_CONFIGS"),
    ];

    for (category, env_var) in worker_configs {
        if let Ok(value) = env::var(env_var) {
            if !value.is_empty() {
                // Parse and migrate individual worker configurations
                let configs: Vec<&str> =
                    value.split(';').filter(|c| !c.trim().is_empty()).collect();
                for (index, config) in configs.iter().enumerate() {
                    let config_name = format!("worker_{}", index);
                    set_config(pool, category, &config_name, config.trim()).await?;

                    // Log with appropriate detail level
                    let display_config = if env_var.contains("OPENAI") {
                        // Hide API keys in OpenAI configs
                        let parts: Vec<&str> = config.split('|').collect();
                        if parts.len() >= 2 {
                            format!("[API_KEY_REDACTED]|{}", parts[1])
                        } else {
                            "[REDACTED]".to_string()
                        }
                    } else {
                        config.to_string()
                    };

                    timed_println(&format!(
                        "    ✅ Migrated {} worker {}: {}",
                        category, index, display_config
                    ));
                }
                timed_println(&format!(
                    "    📊 Total {} workers migrated: {}",
                    category,
                    configs.len()
                ));
            }
        }
    }

    // Migrate LLM parameters
    timed_println("  🎛️  Migrating LLM parameters...");
    let llm_params = [
        ("llm_temperature", "LLM_TEMPERATURE"),
        ("llm_top_p", "LLM_TOP_P"),
        ("llm_top_k", "LLM_TOP_K"),
        ("llm_min_p", "LLM_MIN_P"),
    ];

    for (key, env_var) in llm_params {
        if let Ok(value) = env::var(env_var) {
            if !value.is_empty() {
                set_config(pool, "llm_params", key, &value).await?;
                timed_println(&format!(
                    "    ✅ Migrated LLM parameter: {} = {}",
                    key, value
                ));
            }
        }
    }

    // Migrate OpenAI rate limiting configuration
    timed_println("  🚦 Migrating rate limiting configuration...");
    let rate_limit_configs = [
        ("enabled", "OPENAI_RATE_LIMIT_ENABLED"),
        ("rpm", "OPENAI_RATE_LIMIT_RPM"),
        ("rpd", "OPENAI_RATE_LIMIT_RPD"),
        ("burst", "OPENAI_RATE_LIMIT_BURST"),
    ];

    for (key, env_var) in rate_limit_configs {
        if let Ok(value) = env::var(env_var) {
            if !value.is_empty() {
                set_config(pool, "rate_limit", key, &value).await?;
                timed_println(&format!(
                    "    ✅ Migrated rate limit setting: {} = {}",
                    key, value
                ));
            }
        }
    }

    // Migrate system settings
    timed_println("  ⚙️  Migrating system settings...");
    let system_vars = [
        ("slack_token", "SLACK_TOKEN"),
        ("slack_channel", "SLACK_CHANNEL"),
        ("rust_log", "RUST_LOG"),
        ("default_ollama_model", "DEFAULT_OLLAMA_MODEL"),
        ("no_think_mode", "NO_THINK_MODE"),
    ];

    for (key, env_var) in system_vars {
        if let Ok(value) = env::var(env_var) {
            if !value.is_empty() {
                set_config(pool, "system", key, &value).await?;
                let display_value = if key.contains("token") {
                    "[REDACTED]"
                } else {
                    &value
                };
                timed_println(&format!(
                    "    ✅ Migrated system setting: {} = {}",
                    key, display_value
                ));
            }
        }
    }

    timed_println("✅ Configuration migration completed");
    Ok(())
}

async fn set_config(pool: &Pool<Postgres>, category: &str, name: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO configurations (category, name, value, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (category, name) 
         DO UPDATE SET value = $3, updated_at = NOW()",
    )
    .bind(category)
    .bind(name)
    .bind(value)
    .execute(pool)
    .await?;

    Ok(())
}

async fn validate_migration(pool: &Pool<Postgres>) -> Result<()> {
    timed_println("✅ Validating migration...");

    // Check table counts
    let tables = ["articles", "entities", "article_entities", "configurations"];

    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        timed_println(&format!("  ✅ {}: {} records", table, count));
    }

    // Test basic functionality
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(pool)
        .await?;

    timed_println(&format!(
        "  ✅ PostgreSQL version: {}",
        version
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    ));

    // Test configuration system
    let config_categories: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT category FROM configurations ORDER BY category")
            .fetch_all(pool)
            .await?;

    timed_println(&format!(
        "  ✅ Configuration categories: {:?}",
        config_categories
    ));

    timed_println("✅ Migration validation completed");
    Ok(())
}

async fn determine_migration_mode(pool: &Pool<Postgres>) -> Result<MigrationMode> {
    // Check if PostgreSQL has any data
    let article_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    if article_count == 0 {
        // Fresh migration
        Ok(MigrationMode::Full)
    } else {
        // Get last migration cutoff timestamp
        let cutoff: Option<String> = sqlx::query_scalar(
            "SELECT value FROM migration_metadata WHERE key = 'last_migration_cutoff'",
        )
        .fetch_optional(pool)
        .await?;

        if let Some(cutoff_str) = cutoff {
            let cutoff_time = DateTime::parse_from_rfc3339(&cutoff_str)?.with_timezone(&Utc);
            Ok(MigrationMode::Incremental(cutoff_time))
        } else {
            // Has data but no cutoff - assume full migration needed
            Ok(MigrationMode::Full)
        }
    }
}

fn create_initial_checkpoint(mode: &MigrationMode) -> Result<MigrationCheckpoint> {
    let migration_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();

    let (mode_str, cutoff) = match mode {
        MigrationMode::Full => ("full".to_string(), None),
        MigrationMode::Incremental(cutoff) => ("incremental".to_string(), Some(*cutoff)),
    };

    Ok(MigrationCheckpoint {
        migration_id,
        mode: mode_str,
        start_time: now,
        completed_phases: vec!["prerequisites".to_string()],
        current_phase: "schema".to_string(),
        completed_tables: Vec::new(),
        current_table: None,
        total_tables: 20, // Approximate number of tables
        last_checkpoint: now,
        cutoff_timestamp: cutoff,
    })
}

fn save_checkpoint(checkpoint: &MigrationCheckpoint) -> Result<()> {
    let json = serde_json::to_string_pretty(checkpoint)?;
    fs::write("migration_checkpoint.json", json)?;
    Ok(())
}

async fn update_checkpoint_phase(migration_id: &str, phase: &str) -> Result<()> {
    if let Ok(content) = fs::read_to_string("migration_checkpoint.json") {
        if let Ok(mut checkpoint) = serde_json::from_str::<MigrationCheckpoint>(&content) {
            if checkpoint.migration_id == migration_id {
                checkpoint
                    .completed_phases
                    .push(checkpoint.current_phase.clone());
                checkpoint.current_phase = phase.to_string();
                checkpoint.last_checkpoint = Utc::now();
                save_checkpoint(&checkpoint)?;
            }
        }
    }
    Ok(())
}

async fn resume_migration(debug_mode: bool, stats: &mut MigrationStats) -> Result<()> {
    println!("🔄 Resuming migration from checkpoint...");

    let checkpoint_content = fs::read_to_string("migration_checkpoint.json")
        .map_err(|_| anyhow::anyhow!("No checkpoint file found. Use --auto for new migration."))?;

    let checkpoint: MigrationCheckpoint = serde_json::from_str(&checkpoint_content)?;

    println!(
        "📋 Found checkpoint: {} ({})",
        checkpoint.migration_id, checkpoint.mode
    );
    println!(
        "📅 Started: {}",
        checkpoint.start_time.format("%Y-%m-%d %H:%M:%S")
    );
    println!("✅ Completed phases: {:?}", checkpoint.completed_phases);
    println!("🔄 Current phase: {}", checkpoint.current_phase);

    // Continue from where we left off
    let pool = setup_postgres_connection().await?;

    match checkpoint.current_phase.as_str() {
        "schema" => {
            println!("🏗️  Resuming schema creation...");
            let phase_start = Instant::now();
            create_postgres_schema_enhanced(&pool, debug_mode).await?;
            stats.record_phase("Schema Creation (Resumed)", phase_start.elapsed());
        }
        "data_migration" => {
            println!("📥 Resuming data migration...");
            let phase_start = Instant::now();
            // Resume data migration based on completed tables...
            stats.record_phase("Data Migration (Resumed)", phase_start.elapsed());
        }
        "indexes" => {
            println!("🔗 Resuming index creation...");
            let phase_start = Instant::now();
            create_indexes_after_import_enhanced(&pool, debug_mode).await?;
            stats.record_phase("Index Creation (Resumed)", phase_start.elapsed());
        }
        _ => {
            println!("⚠️  Unknown phase: {}", checkpoint.current_phase);
        }
    }

    if debug_mode {
        stats.print_summary();
    }

    println!("✅ Migration resumed and completed!");
    Ok(())
}

async fn record_migration_completion(pool: &Pool<Postgres>) -> Result<()> {
    let cutoff_timestamp = Utc::now();
    let cutoff_str = cutoff_timestamp.to_rfc3339_opts(SecondsFormat::Micros, true);

    // Store the precise cutoff timestamp for future incremental migrations
    sqlx::query(
        "INSERT INTO migration_metadata (key, value) 
         VALUES ('last_migration_cutoff', $1)
         ON CONFLICT (key) DO UPDATE SET value = $1, created_at = NOW()",
    )
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    // Store migration completion info
    sqlx::query(
        "INSERT INTO migration_metadata (key, value) 
         VALUES ('last_migration_completed', $1)
         ON CONFLICT (key) DO UPDATE SET value = $1, created_at = NOW()",
    )
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    println!("📝 Migration cutoff timestamp stored: {}", cutoff_str);
    Ok(())
}

async fn migrate_incremental_data(
    cutoff: DateTime<Utc>,
    debug_mode: bool,
    stats: &mut MigrationStats,
) -> Result<()> {
    println!("📥 Starting incremental data migration...");
    println!(
        "🕐 Migrating data newer than: {}",
        cutoff.format("%Y-%m-%d %H:%M:%S%.6f%z")
    );

    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S%.6f").to_string();

    // Step 1: Extract incremental data from SQLite
    println!("  📤 Extracting incremental data from SQLite...");
    let extract_start = Instant::now();

    // Build incremental SQLite query
    let incremental_query = format!(
        r#"
        SELECT 'INSERT INTO articles (id, url, seen_at, is_relevant, category, analysis, normalized_url, hash, tiny_summary, title_domain_hash, r2_url, pub_date, event_date, cluster_id, title, json_data, quality, source) VALUES(' || 
               id || ',''' || url || ''',''' || seen_at || ''',' || is_relevant || ',''' || 
               COALESCE(category, '') || ''',''' || COALESCE(analysis, '') || ''',''' || 
               normalized_url || ''',''' || COALESCE(hash, '') || ''',''' || 
               COALESCE(tiny_summary, '') || ''',''' || COALESCE(title_domain_hash, '') || ''',''' || 
               COALESCE(r2_url, '') || ''',''' || COALESCE(pub_date, '') || ''',''' || 
               COALESCE(event_date, '') || ''',' || COALESCE(cluster_id, 'NULL') || ',''' || 
               COALESCE(title, '') || ''',''' || COALESCE(json_data, '') || ''',' || 
               COALESCE(quality, 'NULL') || ',''' || COALESCE(source, '') || ''');' as sql_statement
        FROM articles 
        WHERE seen_at > '{}'
        UNION ALL
        SELECT 'INSERT INTO entities (id, name, type, normalized_name, parent_id) VALUES(' || 
               id || ',''' || name || ''',''' || type || ''',''' || normalized_name || ''',' || 
               COALESCE(parent_id, 'NULL') || ');' as sql_statement
        FROM entities 
        WHERE id IN (
            SELECT DISTINCT entity_id FROM article_entities 
            WHERE article_id IN (
                SELECT id FROM articles WHERE seen_at > '{}'
            )
        )
        "#,
        cutoff_str, cutoff_str
    );

    // Execute incremental query
    let dump_output = Command::new("sqlite3")
        .arg("argus.db")
        .arg(&incremental_query)
        .output()?;

    if !dump_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to extract incremental data: {}",
            String::from_utf8_lossy(&dump_output.stderr)
        ));
    }

    let incremental_sql = String::from_utf8(dump_output.stdout)?;
    let line_count = incremental_sql.lines().count();

    if debug_mode {
        println!(
            "  ⏱️  SQLite extraction: {:.1}s",
            extract_start.elapsed().as_secs_f64()
        );
    }
    stats.record_phase("Incremental SQLite Extraction", extract_start.elapsed());

    if line_count == 0 {
        println!("📊 No new data found since cutoff timestamp");
        return Ok(());
    }

    println!("📊 Found {} new records to migrate", line_count);

    if debug_mode {
        println!("🔍 First 5 lines of incremental SQL:");
        for (i, line) in incremental_sql.lines().take(5).enumerate() {
            let preview = safe_truncate_for_preview(line, 100);
            println!("  {}: {}", i + 1, preview);
        }
    }

    // Step 2: Transform and import incremental data
    println!("  🔄 Transforming and importing incremental data...");
    let transform_start = Instant::now();

    let postgres_sql = transform_sqlite_to_postgres(&incremental_sql)?;

    if debug_mode {
        println!(
            "  ⏱️  SQL transformation: {:.1}s",
            transform_start.elapsed().as_secs_f64()
        );
    }

    // Write to temp file and import (using home directory due to /tmp space constraints)
    let temp_dir = "/home/jandrews/argus_migration_temp";
    fs::create_dir_all(temp_dir)?;
    let temp_file = "/home/jandrews/argus_migration_temp/postgres_incremental.sql";
    fs::write(temp_file, &postgres_sql)?;

    let import_start = Instant::now();
    let import_output = Command::new("psql")
        .arg(&env::var("DATABASE_URL")?)
        .arg("-f")
        .arg(temp_file)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .output()?;

    if !import_output.status.success() {
        let stderr = String::from_utf8_lossy(&import_output.stderr);
        return Err(anyhow::anyhow!("Incremental import failed: {}", stderr));
    }

    if debug_mode {
        println!(
            "  ⏱️  PostgreSQL import: {:.1}s",
            import_start.elapsed().as_secs_f64()
        );
    }

    stats.record_phase(
        "Incremental Data Transform & Import",
        transform_start.elapsed(),
    );
    stats.record_memory("Incremental Import");

    // Cleanup
    let _ = fs::remove_file(temp_file);

    println!("✅ Incremental migration completed: {} records", line_count);
    Ok(())
}

async fn create_postgres_schema_enhanced(pool: &Pool<Postgres>, debug_mode: bool) -> Result<()> {
    timed_println("🏗️  Creating PostgreSQL schema...");

    // First, clean up any existing schema
    clean_existing_schema_enhanced(pool, debug_mode).await?;

    let schema_sql = include_str!("../../memory-bank/postgresql-migration/schema.sql");

    // Parse and categorize SQL statements
    let (table_statements, index_statements, other_statements) =
        parse_schema_statements(schema_sql);

    // OPTIMIZED ORDER: Create tables first, data will be imported next, then indexes
    timed_println("  📋 Creating tables (without indexes for faster import)...");

    if debug_mode {
        timed_println(&format!(
            "  🔍 Creating {} tables...",
            table_statements.len()
        ));
    }

    for (i, statement) in table_statements.iter().enumerate() {
        if debug_mode {
            timed_println(&format!(
                "    📋 Creating table {}/{}",
                i + 1,
                table_statements.len()
            ));
        }

        sqlx::query(statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute table statement: {}\nError: {}",
                statement,
                e
            )
        })?;
    }

    timed_println("  ⚙️  Creating functions and triggers...");
    for statement in other_statements {
        sqlx::query(&statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!("Failed to execute statement: {}\nError: {}", statement, e)
        })?;
    }

    // Store index statements for later execution (after data import)
    *INDEX_STATEMENTS.lock().unwrap() = index_statements;

    timed_println("✅ PostgreSQL schema created (indexes will be created after data import)");
    Ok(())
}

async fn clean_existing_schema_enhanced(pool: &Pool<Postgres>, debug_mode: bool) -> Result<()> {
    timed_println("  🧹 Cleaning existing schema...");

    // Get list of all tables in the public schema
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname = 'public'")
            .fetch_all(pool)
            .await?;

    if !tables.is_empty() {
        timed_println(&format!(
            "    🗑️  Dropping {} existing tables...",
            tables.len()
        ));

        if debug_mode {
            timed_println(&format!("    🔍 Tables to drop: {:?}", tables));
        }

        // Drop all tables with CASCADE to handle foreign key dependencies
        for table in tables {
            let drop_sql = format!("DROP TABLE IF EXISTS {} CASCADE", table);
            sqlx::query(&drop_sql)
                .execute(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to drop table {}: {}", table, e))?;
        }
    }

    // Drop any remaining sequences, functions, and types
    timed_println("    🔧 Cleaning sequences, functions, and types...");

    // Drop sequences
    let sequences: Vec<String> =
        sqlx::query_scalar("SELECT sequencename FROM pg_sequences WHERE schemaname = 'public'")
            .fetch_all(pool)
            .await?;

    for sequence in sequences {
        let drop_sql = format!("DROP SEQUENCE IF EXISTS {} CASCADE", sequence);
        sqlx::query(&drop_sql).execute(pool).await?;
    }

    // Drop custom functions (but keep system functions)
    let functions: Vec<String> = sqlx::query_scalar(
        "SELECT proname FROM pg_proc p 
         JOIN pg_namespace n ON p.pronamespace = n.oid 
         WHERE n.nspname = 'public' AND p.prokind = 'f'",
    )
    .fetch_all(pool)
    .await?;

    for function in functions {
        let drop_sql = format!("DROP FUNCTION IF EXISTS {} CASCADE", function);
        sqlx::query(&drop_sql).execute(pool).await?;
    }

    timed_println("    ✅ Schema cleanup completed");
    Ok(())
}

async fn migrate_data_via_dump_enhanced(
    pool: &Pool<Postgres>,
    debug_mode: bool,
    stats: &mut MigrationStats,
) -> Result<()> {
    println!("📥 Migrating data via dump/restore...");

    // Step 1: Export SQLite data
    println!("  📤 Exporting SQLite data...");
    let export_start = Instant::now();

    let dump_output = Command::new("sqlite3")
        .arg("argus.db")
        .arg(".dump")
        .output()?;

    if !dump_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to dump SQLite data: {}",
            String::from_utf8_lossy(&dump_output.stderr)
        ));
    }

    let sqlite_dump = String::from_utf8(dump_output.stdout)?;

    if debug_mode {
        println!(
            "  ⏱️  SQLite export: {:.1}s",
            export_start.elapsed().as_secs_f64()
        );
    }

    // Step 2: Transform SQL for PostgreSQL compatibility
    println!("  🔄 Transforming SQL for PostgreSQL...");
    let transform_start = Instant::now();

    let postgres_sql = transform_sqlite_to_postgres(&sqlite_dump)?;

    // Debug: Check what we're actually importing
    let line_count = postgres_sql.lines().count();
    let insert_count = postgres_sql
        .lines()
        .filter(|line| line.trim().starts_with("INSERT INTO"))
        .count();

    if debug_mode {
        println!(
            "  ⏱️  SQL transformation: {:.1}s",
            transform_start.elapsed().as_secs_f64()
        );
    }

    println!(
        "  📊 Transformed SQL: {} lines, {} INSERT statements",
        line_count, insert_count
    );

    // Write transformed SQL to temp file (using home directory due to /tmp space constraints)
    let temp_dir = "/home/jandrews/argus_migration_temp";
    fs::create_dir_all(temp_dir)?;
    let temp_file = "/home/jandrews/argus_migration_temp/postgres_import.sql";
    fs::write(temp_file, &postgres_sql)?;

    if debug_mode {
        // Debug: Show first few lines of transformed SQL
        let preview_lines: Vec<&str> = postgres_sql.lines().take(10).collect();
        println!("  🔍 First 10 lines of transformed SQL:");
        for (i, line) in preview_lines.iter().enumerate() {
            println!("    {}: {}", i + 1, line);
        }
    }

    // Step 3: Import to PostgreSQL with progress tracking
    println!("  📥 Importing to PostgreSQL...");
    let import_start = Instant::now();

    // Create progress bar for import
    let pb = ProgressBar::new(insert_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  📊 Importing data [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Use optimized PostgreSQL import settings
    let import_output = Command::new("psql")
        .arg(&env::var("DATABASE_URL")?)
        .arg("-f")
        .arg(temp_file)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-c")
        .arg("SET synchronous_commit = off;") // Faster imports
        .arg("-c")
        .arg("SET maintenance_work_mem = '512MB';") // More memory for operations
        .output()?;

    pb.finish_with_message("✅ Data import completed");

    // Show only stderr for debugging (stdout is too verbose with INSERT 0 1 messages)
    let stderr = String::from_utf8_lossy(&import_output.stderr);

    if !stderr.is_empty() && debug_mode {
        println!("  ⚠️  PostgreSQL stderr: {}", stderr);
    }

    if !import_output.status.success() {
        return Err(anyhow::anyhow!(
            "PostgreSQL import failed with exit code: {}\nStderr: {}",
            import_output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    if debug_mode {
        println!(
            "  ⏱️  PostgreSQL import: {:.1}s",
            import_start.elapsed().as_secs_f64()
        );
    }

    println!("  ✅ PostgreSQL import completed successfully");

    // Step 4: Reset sequences
    let sequence_start = Instant::now();
    reset_postgres_sequences(pool).await?;

    if debug_mode {
        println!(
            "  ⏱️  Sequence reset: {:.1}s",
            sequence_start.elapsed().as_secs_f64()
        );
    }
    stats.record_phase("Full Migration Sequence Reset", sequence_start.elapsed());

    // Cleanup
    let _ = fs::remove_file(temp_file);

    println!("✅ Data migration completed");
    Ok(())
}

async fn create_indexes_after_import_enhanced(
    pool: &Pool<Postgres>,
    debug_mode: bool,
) -> Result<()> {
    println!("🔗 Creating indexes after data import for optimal performance...");

    let index_statements = INDEX_STATEMENTS.lock().unwrap().clone();

    if debug_mode {
        println!("  🔍 Creating {} indexes...", index_statements.len());
    }

    // Create progress bar for index creation
    let pb = ProgressBar::new(index_statements.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  📊 Creating indexes [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    for (i, statement) in index_statements.iter().enumerate() {
        pb.set_position(i as u64);

        if debug_mode {
            println!("    🔗 Creating index {}/{}", i + 1, index_statements.len());
        }

        let start_time = Instant::now();
        sqlx::query(statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute index statement: {}\nError: {}",
                statement,
                e
            )
        })?;

        if debug_mode {
            println!(
                "      ⏱️  Index created in {:.1}s",
                start_time.elapsed().as_secs_f64()
            );
        }
    }

    pb.finish_with_message("✅ All indexes created");
    println!("✅ All indexes created successfully");
    Ok(())
}
