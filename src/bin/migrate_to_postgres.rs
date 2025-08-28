//! PostgreSQL Migration Tool
//!
//! This tool provides a direct migration path from SQLite to PostgreSQL by:
//! 1. Creating a PostgreSQL schema that closely matches SQLite
//! 2. Direct 7-column mapping with minimal transformation
//! 3. No complex column mapping or NULL generation
//!
//! This approach prioritizes getting off SQLite quickly rather than
//! implementing complex schema changes during migration. Additional columns can
//! be added later via ALTER TABLE statements once the migration is complete.

use anyhow::Result;
use chrono::{DateTime, Local};
use clap::{Arg, Command as ClapCommand};
use regex::Regex;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use std::time::Instant;
use tokio::time::Duration;

fn timestamp() -> String {
    let now: DateTime<Local> = Local::now();
    format!("[{}]", now.format("%Y-%m-%d %H:%M:%S"))
}

fn extract_table_name_from_insert(insert_stmt: &str, re: &regex::Regex) -> Option<String> {
    // Parse INSERT INTO table_name ... to extract table_name
    if let Some(captures) = re.captures(insert_stmt) {
        return Some(captures[1].to_string());
    }
    None
}

fn format_progress_bar(current: usize, total: usize, width: usize) -> String {
    let percentage = if total > 0 {
        (current * 100) / total
    } else {
        0
    };
    let filled = if total > 0 {
        (current * width) / total
    } else {
        0
    };
    let empty = width.saturating_sub(filled);

    format!(
        "[{}{}] {}%",
        "=".repeat(filled),
        "-".repeat(empty),
        percentage
    )
}

fn get_memory_usage() -> Result<String> {
    // Try to get memory usage on Linux systems
    if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
        for line in contents.lines() {
            if line.starts_with("VmRSS:") {
                // Extract RSS memory usage
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(format!("{}kB RAM", parts[1]));
                }
            }
        }
    }

    // Fallback - just return a placeholder
    Ok("N/A".to_string())
}

fn format_memory_stats(processed_lines: usize, insert_count: usize) -> String {
    match get_memory_usage() {
        Ok(memory) => format!(
            "📊 Memory: {} | Lines: {} | Inserts: {}",
            memory, processed_lines, insert_count
        ),
        Err(_) => format!("📊 Lines: {} | Inserts: {}", processed_lines, insert_count),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = ClapCommand::new("migrate_to_postgres")
        .about("PostgreSQL migration - direct column mapping ")
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable detailed debugging output ")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let debug_mode = matches.get_flag("debug");

    println!("🎯 Starting PostgreSQL migration from SQLite...");
    println!("🔧 Direct column mapping (SQLite -> PostgreSQL)");
    println!();

    // Step 1: Prerequisites
    check_prerequisites().await?;

    // Step 2: Setup PostgreSQL
    let pool = setup_postgres_connection().await?;

    // Step 3: Create simplified schema
    create_postgres_schema(&pool).await?;

    // Step 4: Migrate data with direct mapping
    migrate_data_direct_mapping(&pool, debug_mode).await?;

    // Step 5: Basic validation
    validate_migration(&pool).await?;

    println!("✅ Migration completed successfully!");
    println!("📝 Schema uses direct 7-column mapping from SQLite ");
    println!("🚀 You can now test with: cargo run --release ");
    println!();
    println!("💡 Additional columns can be added later via ALTER TABLE statements ");

    Ok(())
}

async fn check_prerequisites() -> Result<()> {
    println!("🔍 Checking prerequisites...");

    // Check SQLite database exists
    if !std::path::Path::new("argus.db").exists() {
        return Err(anyhow::anyhow!("SQLite database argus.db not found"));
    }

    // Check DATABASE_URL is set
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set "))?;

    // Validate that it's a PostgreSQL URL
    if !database_url.starts_with("postgresql://") && !database_url.starts_with("postgres://") {
        return Err(anyhow::anyhow!(
            "DATABASE_URL must be a PostgreSQL connection string"
        ));
    }

    println!("✅ Prerequisites check passed");
    Ok(())
}

async fn setup_postgres_connection() -> Result<Pool<Postgres>> {
    println!("🔌 Setting up PostgreSQL connection...");

    let database_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&database_url)
        .await?;

    // Test connection
    sqlx::query("SELECT 1").execute(&pool).await?;
    println!("✅ PostgreSQL connection established");

    Ok(pool)
}

async fn create_postgres_schema(pool: &Pool<Postgres>) -> Result<()> {
    println!("🏗️  Creating simplified PostgreSQL schema...");
    println!("📋 Converting SQLite schema to PostgreSQL with direct column mapping");

    // Drop existing tables if they exist (in reverse order due to foreign keys)
    println!("  🧹 Cleaning existing tables...");

    // Instead of dropping the entire schema, drop individual tables safely
    let cleanup_tables = vec![
        "alias_cache_stats",
        "alias_review_items",
        "alias_review_batches",
        "alias_pattern_stats",
        "entity_negative_matches",
        "entity_aliases",
        "ip_logs",
        "device_subscriptions",
        "devices",
        "life_safety_queue",
        "matched_topics_queue",
        "rss_queue",
        "endpoint_alerts",
        "endpoint_timeout_events",
        "cluster_merge_history",
        "article_cluster_mappings",
        "article_clusters",
        "article_entities",
        "entities",
        "articles",
    ];

    for table in cleanup_tables {
        let drop_sql = format!("DROP TABLE IF EXISTS {} CASCADE", table);
        let _ = sqlx::query(&drop_sql).execute(pool).await; // Ignore errors
    }

    // Execute schema statements individually to avoid multi-statement issues
    let schema_statements = vec![
        // Core Articles Table
        "CREATE TABLE articles (
            id BIGSERIAL PRIMARY KEY,
            url TEXT NOT NULL,
            normalized_url TEXT NOT NULL UNIQUE,
            seen_at TIMESTAMPTZ NOT NULL,
            pub_date TIMESTAMPTZ,
            event_date TIMESTAMPTZ,
            title TEXT,
            source TEXT,
            is_relevant BOOLEAN NOT NULL,
            category TEXT,
            tiny_summary TEXT,
            analysis TEXT,
            json_data TEXT,
            quality REAL,
            hash TEXT,
            title_domain_hash TEXT,
            r2_url TEXT,
            cluster_id BIGINT
        )",
        "CREATE INDEX idx_relevant_category ON articles (is_relevant, category)",
        "CREATE INDEX idx_hash ON articles (hash)",
        "CREATE INDEX idx_title_domain_hash ON articles (title_domain_hash)",
        "CREATE INDEX idx_r2_url ON articles (r2_url)",
        "CREATE INDEX idx_seen_at_r2_url ON articles (seen_at, r2_url)",
        "CREATE INDEX idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url)",
        "CREATE INDEX idx_articles_event_date ON articles (event_date)",
        "CREATE INDEX idx_articles_pub_date ON articles (pub_date)",
        "CREATE INDEX idx_articles_cluster_id ON articles (cluster_id)",
        // Entity tables
        "CREATE TABLE entities (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            parent_id BIGINT,
            UNIQUE(normalized_name, type),
            FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
        )",
        "CREATE INDEX idx_entities_normalized_name ON entities (normalized_name)",
        "CREATE INDEX idx_entities_type ON entities (type)",
        "CREATE INDEX idx_entities_parent_id ON entities (parent_id)",
        // Queue tables
        "CREATE TABLE rss_queue (
            id BIGSERIAL PRIMARY KEY,
            url TEXT NOT NULL,
            normalized_url TEXT NOT NULL UNIQUE,
            title TEXT,
            seen_at TIMESTAMPTZ NOT NULL,
            pub_date TIMESTAMPTZ
        )",
        "CREATE UNIQUE INDEX idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url)",
        "CREATE UNIQUE INDEX idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url)",
    ];

    // Execute each statement individually
    for statement in schema_statements {
        sqlx::query(statement).execute(pool).await?;
    }

    println!(
        "{} ✅ Complete PostgreSQL schema created (core tables)",
        timestamp()
    );

    Ok(())
}

async fn migrate_data_direct_mapping(pool: &Pool<Postgres>, debug_mode: bool) -> Result<()> {
    println!(
        "{} 📥 Migrating data with streaming approach (memory-optimized)...",
        timestamp()
    );

    // Step 1: Get all table names that exist in SQLite
    println!(
        "{} 🔍 Discovering tables in SQLite database...",
        timestamp()
    );
    let tables_output = Command::new("sqlite3")
        .arg("argus.db")
        .arg("SELECT name FROM sqlite_master WHERE type='table' AND name != 'sqlite_sequence';")
        .output()?;

    if !tables_output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get table list: {}",
            String::from_utf8_lossy(&tables_output.stderr)
        ));
    }

    let tables: Vec<String> = String::from_utf8(tables_output.stdout)?
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    println!(
        "{} 📋 Found {} tables to migrate: {:?}",
        timestamp(),
        tables.len(),
        tables
    );

    if tables.is_empty() {
        println!("{} ⚠️  No tables found in SQLite database", timestamp());
        return Ok(());
    }

    // Step 2: Show per-table progress before bulk extraction
    println!("{} 📤 Extracting data from SQLite tables...", timestamp());

    // Process each table individually to show progress
    for table in &tables {
        println!("{} 📦 analyzing {}...", timestamp(), table);

        // Get row count for this table to show progress
        let count_output = Command::new("sqlite3")
            .arg("argus.db")
            .arg(&format!("SELECT COUNT(*) FROM {};", table))
            .output()?;

        if count_output.status.success() {
            if let Ok(count_str) = String::from_utf8(count_output.stdout) {
                if let Ok(count) = count_str.trim().parse::<i32>() {
                    if count > 0 {
                        println!("{} 📊 {} has {} rows to migrate", timestamp(), table, count);
                    } else {
                        println!("{} 📭 {} is empty, skipping", timestamp(), table);
                    }
                }
            }
        }
    }

    // Step 3: Use streaming approach to process SQLite dump without loading everything into memory
    println!(
        "{} 🔄 Starting streaming data transformation...",
        timestamp()
    );
    let start_time = Instant::now();

    // Set up streaming SQLite dump process
    let mut dump_process = Command::new("sqlite3")
        .arg("argus.db")
        .arg(".dump")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = dump_process
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdout from sqlite3 process"))?;

    // Set up streaming output to PostgreSQL temp file in current directory
    let temp_file = "./postgres_streaming_import.sql";
    let output_file = std::fs::File::create(temp_file)?;
    let mut writer = BufWriter::new(output_file);

    // Set up buffered reader for processing dump line by line
    let reader = BufReader::new(stdout);

    // Compile regex patterns once for performance
    let insert_regex = Regex::new(r"^INSERT\s+INTO\s+`?([a-zA-Z0-9_]+)`?").unwrap();
    let timestamp_regex = Regex::new(r"'(\d{10})'").unwrap();

    let mut processed_lines = 0;
    let mut insert_count = 0;
    let mut current_table = String::new();
    const BATCH_SIZE: usize = 1000; // Process in batches to control memory
    let mut batch_buffer: Vec<String> = Vec::with_capacity(BATCH_SIZE);

    // State tracking for multi-line CREATE statements
    let mut inside_create_statement = false;

    println!("{} 🔄 Processing dump stream...", timestamp());

    // Process dump line by line
    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        processed_lines += 1;

        // Show progress every 50,000 lines with memory usage
        if processed_lines % 50000 == 0 {
            println!(
                "{} 🔄 {}",
                timestamp(),
                format_memory_stats(processed_lines, insert_count)
            );
        }

        // Check if starting a CREATE statement (multi-line)
        if trimmed.starts_with("CREATE TABLE")
            || trimmed.starts_with("CREATE INDEX")
            || trimmed.starts_with("CREATE UNIQUE INDEX")
        {
            inside_create_statement = true;
            continue;
        }

        // Check if ending a CREATE statement
        if inside_create_statement && trimmed.ends_with(");") {
            inside_create_statement = false;
            continue;
        }

        // Skip if inside CREATE statement
        if inside_create_statement {
            continue;
        }

        // Skip other SQLite-specific statements
        if trimmed.starts_with("PRAGMA")
            || trimmed.starts_with("BEGIN TRANSACTION")
            || trimmed.starts_with("COMMIT")
            || trimmed.contains("sqlite_sequence")
            || trimmed.is_empty()
        {
            continue;
        }

        // Process INSERT statements
        if trimmed.starts_with("INSERT INTO") {
            // Extract table name for progress tracking
            if let Some(table_name) = extract_table_name_from_insert(trimmed, &insert_regex) {
                if table_name != current_table {
                    if !current_table.is_empty() && !batch_buffer.is_empty() {
                        // Write previous table's remaining batch
                        for stmt in &batch_buffer {
                            writeln!(writer, "{}", stmt)?;
                        }
                        batch_buffer.clear();
                        println!("{} ✅ Completed table: {}", timestamp(), current_table);
                    }
                    current_table = table_name.clone();
                    println!("{} 🔄 Processing table: {}", timestamp(), current_table);
                }
            }

            // Transform the INSERT statement for PostgreSQL compatibility
            let mut result = line.clone();

            // Convert Unix timestamps to PostgreSQL format
            result = timestamp_regex
                .replace_all(&result, |caps: &regex::Captures| {
                    let unix_timestamp = &caps[1];
                    if let Ok(timestamp) = unix_timestamp.parse::<i64>() {
                        if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                            format!("'{}'", datetime.format("%Y-%m-%d %H:%M:%S%z"))
                        } else {
                            format!("'{}'", unix_timestamp)
                        }
                    } else {
                        format!("'{}'", unix_timestamp)
                    }
                })
                .to_string();

            // Convert boolean values (SQLite uses 1/0, PostgreSQL uses true/false)
            // Use regex to be very specific and avoid converting ID or other integer values
            // Only convert standalone 1/0 values that are clearly boolean (between commas or at end)
            let boolean_regex = Regex::new(r",([01])([,)])").unwrap();
            result = boolean_regex
                .replace_all(&result, |caps: &regex::Captures| {
                    let boolean_val = if &caps[1] == "1" { "true" } else { "false" };
                    format!(",{}{}", boolean_val, &caps[2])
                })
                .to_string();

            // Add to batch buffer
            batch_buffer.push(result);
            insert_count += 1;

            // Write batch when it reaches batch size
            if batch_buffer.len() >= BATCH_SIZE {
                for stmt in &batch_buffer {
                    writeln!(writer, "{}", stmt)?;
                }
                batch_buffer.clear();
            }
        } else if !trimmed.starts_with("INSERT INTO") && !trimmed.is_empty() {
            // Handle other statements (write immediately, they're usually few)
            writeln!(writer, "{}", line)?;
        }
    }

    // Write remaining batch
    if !batch_buffer.is_empty() {
        for stmt in &batch_buffer {
            writeln!(writer, "{}", stmt)?;
        }
        println!("{} ✅ Completed table: {}", timestamp(), current_table);
    }

    // Ensure all data is written to disk
    writer.flush()?;
    drop(writer); // Close the file

    // Wait for sqlite3 process to complete
    let dump_status = dump_process.wait()?;
    if !dump_status.success() {
        return Err(anyhow::anyhow!("SQLite dump process failed"));
    }

    println!(
        "{} ✅ Streaming transformation completed: {} lines processed, {} INSERT statements",
        timestamp(),
        processed_lines,
        insert_count
    );

    if debug_mode {
        println!(
            "{} ⏱️  Streaming transformation: {:.1}s",
            timestamp(),
            start_time.elapsed().as_secs_f64()
        );
    }

    // Step 4: Import to PostgreSQL using streaming approach
    println!("{} 📥 Importing to PostgreSQL...", timestamp());
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
        return Err(anyhow::anyhow!("PostgreSQL import failed: {}", stderr));
    }

    if debug_mode {
        println!(
            "{} ⏱️  Import: {:.1}s",
            timestamp(),
            import_start.elapsed().as_secs_f64()
        );
    }

    // Cleanup
    let _ = fs::remove_file(temp_file);

    // Step 5: Reset sequences for all tables
    println!("{} 🔢 Resetting PostgreSQL sequences...", timestamp());
    for table in &tables {
        let sequence_name = format!("{}_id_seq", table);
        let reset_sql = format!(
            "SELECT setval('{}', COALESCE((SELECT MAX(id) FROM {}), 1))",
            sequence_name, table
        );

        // Ignore errors for tables that might not have sequences
        let _ = sqlx::query(&reset_sql).execute(pool).await;
    }

    println!(
        "{} ✅ Memory-optimized migration completed: {} tables, {} INSERT statements",
        timestamp(),
        tables.len(),
        insert_count
    );
    println!(
        "{} 💾 Peak memory usage significantly reduced through streaming",
        timestamp()
    );
    Ok(())
}

async fn validate_migration(pool: &Pool<Postgres>) -> Result<()> {
    println!("{} ✅ Validating migration...", timestamp());

    // Count records
    let pg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
        .fetch_one(pool)
        .await?;

    // Get SQLite count for comparison
    let sqlite_output = Command::new("sqlite3")
        .arg("argus.db")
        .arg("SELECT COUNT(*) FROM articles;")
        .output()?;

    let sqlite_count: i64 = String::from_utf8(sqlite_output.stdout)?
        .trim()
        .parse()
        .unwrap_or(0);

    println!("{} 📊 SQLite articles: {}", timestamp(), sqlite_count);
    println!("{} 📊 PostgreSQL articles: {}", timestamp(), pg_count);

    if pg_count == sqlite_count {
        println!("{} ✅ Record counts match", timestamp());
    } else {
        return Err(anyhow::anyhow!(
            "Record count mismatch: SQLite={}, PostgreSQL={}",
            sqlite_count,
            pg_count
        ));
    }

    // Test a few sample records
    let sample: Vec<(i64, String)> = sqlx::query_as("SELECT id, url FROM articles LIMIT 3")
        .fetch_all(pool)
        .await?;

    println!("{} 📋 Sample records:", timestamp());
    for (id, url) in sample {
        println!("{}   {}: {}", timestamp(), id, &url[..url.len().min(50)]);
    }

    println!("{} ✅ Migration validation completed", timestamp());
    Ok(())
}
