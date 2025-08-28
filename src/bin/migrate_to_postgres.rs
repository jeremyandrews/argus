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
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::process::Command;
use std::time::Instant;
use tokio::time::Duration;

fn timestamp() -> String {
    let now: DateTime<Local> = Local::now();
    format!("[{}]", now.format("%Y-%m-%d %H:%M:%S"))
}

fn extract_table_name_from_insert(insert_stmt: &str) -> Option<String> {
    // Parse INSERT INTO table_name ... to extract table_name
    use regex::Regex;
    let re = Regex::new(r"^INSERT\s+INTO\s+`?([a-zA-Z0-9_]+)`?").unwrap();
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

#[tokio::main]
async fn main() -> Result<()> {
    let matches = ClapCommand::new("migrate_to_postgres_simple")
        .about("Simplified PostgreSQL migration - direct 7-column mapping ")
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable detailed debugging output ")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let debug_mode = matches.get_flag("debug");

    println!("🎯 Starting SIMPLIFIED PostgreSQL migration from SQLite...");
    println!("🔧 Direct 7-column mapping (SQLite -> PostgreSQL)");
    println!();

    // Step 1: Prerequisites
    check_prerequisites().await?;

    // Step 2: Setup PostgreSQL
    let pool = setup_postgres_connection().await?;

    // Step 3: Create simplified schema
    create_simple_postgres_schema(&pool).await?;

    // Step 4: Migrate data with direct mapping
    migrate_data_direct_mapping(&pool, debug_mode).await?;

    // Step 5: Basic validation
    validate_simple_migration(&pool).await?;

    println!("✅ Simplified migration completed successfully!");
    println!("📝 Schema uses direct 7-column mapping from SQLite ");
    println!("🚀 You can now test with: cargo run --release ");
    println!();
    println!("💡 Additional columns can be added later via ALTER TABLE statements ");

    Ok(())
}

async fn check_prerequisites() -> Result<()> {
    println!("🔍 Checking prerequisites...");

    // Check SQLite database exists
    if !std::path::Path::new("argus.db ").exists() {
        return Err(anyhow::anyhow!("SQLite database argus.db not found "));
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

async fn create_simple_postgres_schema(pool: &Pool<Postgres>) -> Result<()> {
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
        "{} 📥 Migrating data with direct column mapping...",
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
        println!("{} 📦 dumping {}...", timestamp(), table);

        // Get row count for this table to show progress
        let count_output = Command::new("sqlite3")
            .arg("argus.db")
            .arg(&format!("SELECT COUNT(*) FROM {};", table))
            .output()?;

        if count_output.status.success() {
            if let Ok(count_str) = String::from_utf8(count_output.stdout) {
                if let Ok(count) = count_str.trim().parse::<i32>() {
                    if count > 0 {
                        println!("{} � {} has {} rows to migrate", timestamp(), table, count);
                    } else {
                        println!("{} 📭 {} is empty, skipping", timestamp(), table);
                    }
                }
            }
        }
    }

    // Step 3: Use SQLite .dump to export all data with proper formatting
    println!("{} 🔄 Starting bulk data extraction...", timestamp());
    let start_time = Instant::now();

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
            "{} 📊 Extracted dump in {:.1}s",
            timestamp(),
            start_time.elapsed().as_secs_f64()
        );
    }

    // Step 4: Transform SQL for PostgreSQL compatibility
    println!(
        "{} 🔄 Transforming for PostgreSQL compatibility...",
        timestamp()
    );
    let transform_start = Instant::now();

    // Group INSERT statements by table and process table by table
    let mut table_inserts: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut other_statements = Vec::new();

    // Get total line count for progress reporting
    let dump_lines: Vec<&str> = sqlite_dump.lines().collect();
    let total_lines = dump_lines.len();
    let mut processed_lines = 0;

    println!(
        "{} 📄 Processing {} lines from dump file...",
        timestamp(),
        total_lines
    );

    // First pass: collect and group INSERT statements by table with progress
    for line in dump_lines.iter() {
        let trimmed = line.trim();
        processed_lines += 1;

        // Show progress every 10% or every 50,000 lines (whichever is more frequent)
        let progress_interval = std::cmp::min(total_lines / 10, 50000).max(1);
        if processed_lines % progress_interval == 0 || processed_lines == total_lines {
            let progress_bar = format_progress_bar(processed_lines, total_lines, 30);
            println!(
                "{} 🔄 {} ({}/{} lines)",
                timestamp(),
                progress_bar,
                processed_lines,
                total_lines
            );
        }

        // Skip SQLite-specific statements
        if trimmed.starts_with("PRAGMA")
            || trimmed.starts_with("BEGIN TRANSACTION")
            || trimmed.starts_with("COMMIT")
            || trimmed.contains("sqlite_sequence")
            || trimmed.starts_with("CREATE TABLE")
            || trimmed.starts_with("CREATE INDEX")
            || trimmed.starts_with("CREATE UNIQUE INDEX")
            || trimmed.is_empty()
        {
            continue;
        }

        // Process INSERT statements
        if trimmed.starts_with("INSERT INTO") {
            // Extract table name from INSERT statement
            if let Some(table_name) = extract_table_name_from_insert(trimmed) {
                table_inserts
                    .entry(table_name)
                    .or_insert_with(Vec::new)
                    .push(line.to_string());
            } else {
                other_statements.push(line.to_string());
            }
        } else {
            other_statements.push(line.to_string());
        }
    }

    println!(
        "{} ✅ Completed processing all {} lines",
        timestamp(),
        total_lines
    );

    // Second pass: process each table's INSERT statements with progress reporting
    let mut all_transformed_statements = Vec::new();
    let total_tables = table_inserts.len();
    let mut processed_tables = 0;

    for (table_name, inserts) in table_inserts.iter() {
        processed_tables += 1;
        println!(
            "{} 🔄 transforming {} ({}/{} tables)...",
            timestamp(),
            table_name,
            processed_tables,
            total_tables
        );

        let mut transformed_inserts = Vec::new();
        for insert_stmt in inserts {
            let mut result = insert_stmt.clone();

            // Convert Unix timestamps to PostgreSQL format
            use regex::Regex;
            let timestamp_regex = Regex::new(r"'(\d{10})'").unwrap();

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
            result = result
                .replace(",1,", ",true,")
                .replace(",0,", ",false,")
                .replace("(1,", "(true,")
                .replace("(0,", "(false,")
                .replace(",1)", ",true)")
                .replace(",0)", ",false)");

            transformed_inserts.push(result);
        }

        println!(
            "{} ✅ transformed {} ({} INSERT statements)",
            timestamp(),
            table_name,
            transformed_inserts.len()
        );

        all_transformed_statements.extend(transformed_inserts);
    }

    // Add any other non-INSERT statements
    all_transformed_statements.extend(other_statements);

    let postgres_sql = all_transformed_statements.join("\n");

    let insert_count = postgres_sql
        .lines()
        .filter(|line| line.trim().starts_with("INSERT INTO"))
        .count();

    if debug_mode {
        println!(
            "{} ⏱️  Transformation: {:.1}s",
            timestamp(),
            transform_start.elapsed().as_secs_f64()
        );
        println!(
            "{} 📊 Generated {} INSERT statements",
            timestamp(),
            insert_count
        );
    }

    // Step 5: Import to PostgreSQL
    println!("{} 📥 Importing to PostgreSQL...", timestamp());
    let import_start = Instant::now();

    // Write to temporary file
    let temp_file = "/tmp/postgres_complete_import.sql";
    fs::write(temp_file, &postgres_sql)?;

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

    // Step 6: Reset sequences for all tables
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
        "{} ✅ Data migration completed: {} tables, {} INSERT statements",
        timestamp(),
        tables.len(),
        insert_count
    );
    Ok(())
}

async fn validate_simple_migration(pool: &Pool<Postgres>) -> Result<()> {
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
