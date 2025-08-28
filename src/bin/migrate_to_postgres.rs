//! Simplified PostgreSQL Migration Tool
//!
//! This tool provides a direct migration path from SQLite to PostgreSQL by:
//! 1. Creating a simplified PostgreSQL schema that closely matches SQLite
//! 2. Direct 7-column mapping with minimal transformation
//! 3. No complex column mapping or NULL generation
//!
//! This approach prioritizes getting off SQLite quickly rather than
//! implementing complex schema changes during migration. Additional columns can
//! be added later via ALTER TABLE statements once the migration is complete.

use anyhow::Result;
use clap::{Arg, Command as ClapCommand};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::process::Command;
use std::time::Instant;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = ClapCommand::new("migrate_to_postgres_simple")
        .about("Simplified PostgreSQL migration - direct 7-column mapping")
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Enable detailed debugging output")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let debug_mode = matches.get_flag("debug");

    println!("🎯 Starting SIMPLIFIED PostgreSQL migration from SQLite...");
    println!("🔧 Direct 7-column mapping (SQLite → PostgreSQL)");
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
    println!("📝 Schema uses direct 7-column mapping from SQLite");
    println!("🚀 You can now test with: cargo run --release");
    println!();
    println!("💡 Additional columns can be added later via ALTER TABLE statements");

    Ok(())
}

async fn check_prerequisites() -> Result<()> {
    println!("🔍 Checking prerequisites...");

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
    sqlx::query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(pool)
        .await?;
    sqlx::query("CREATE SCHEMA public").execute(pool).await?;

    // Create all tables from the SQLite schema, converting to PostgreSQL
    // This matches the full schema from src/db/schema.rs but with PostgreSQL types
    let schema = r#"
        -- Core Articles Table (18 columns from schema.rs)
        CREATE TABLE articles (
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
        );

        CREATE INDEX idx_relevant_category ON articles (is_relevant, category);
        CREATE INDEX idx_hash ON articles (hash);
        CREATE INDEX idx_title_domain_hash ON articles (title_domain_hash);
        CREATE INDEX idx_r2_url ON articles (r2_url);
        CREATE INDEX idx_seen_at_r2_url ON articles (seen_at, r2_url);
        CREATE INDEX idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url);
        CREATE INDEX idx_articles_event_date ON articles (event_date);
        CREATE INDEX idx_articles_pub_date ON articles (pub_date);
        CREATE INDEX idx_articles_cluster_id ON articles (cluster_id);

        -- Entity tables for improved article matching
        CREATE TABLE entities (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            parent_id BIGINT,
            UNIQUE(normalized_name, type),
            FOREIGN KEY (parent_id) REFERENCES entities (id) ON DELETE SET NULL
        );

        CREATE INDEX idx_entities_normalized_name ON entities (normalized_name);
        CREATE INDEX idx_entities_type ON entities (type);
        CREATE INDEX idx_entities_parent_id ON entities (parent_id);

        -- Entity-Article relationships
        CREATE TABLE article_entities (
            id BIGSERIAL PRIMARY KEY,
            article_id BIGINT NOT NULL,
            entity_id BIGINT NOT NULL,
            importance TEXT NOT NULL,
            context TEXT,
            FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
            FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE CASCADE,
            UNIQUE(article_id, entity_id)
        );

        CREATE INDEX idx_article_entities_article_id ON article_entities (article_id);
        CREATE INDEX idx_article_entities_entity_id ON article_entities (entity_id);
        CREATE INDEX idx_article_entities_importance ON article_entities (importance);

        -- Article clusters
        CREATE TABLE article_clusters (
            id BIGSERIAL PRIMARY KEY,
            name TEXT,
            creation_date TIMESTAMPTZ NOT NULL,
            last_updated TIMESTAMPTZ NOT NULL,
            primary_entity_ids TEXT NOT NULL DEFAULT '[]',
            article_count INTEGER NOT NULL DEFAULT 0,
            needs_summary_update INTEGER NOT NULL DEFAULT 1,
            summary TEXT,
            summary_version INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            importance_score REAL NOT NULL DEFAULT 0.0,
            has_timeline INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX idx_article_clusters_last_updated ON article_clusters (last_updated);
        CREATE INDEX idx_article_clusters_status ON article_clusters (status);
        CREATE INDEX idx_article_clusters_importance ON article_clusters (importance_score);

        -- Article-cluster relationships
        CREATE TABLE article_cluster_mappings (
            id BIGSERIAL PRIMARY KEY,
            article_id BIGINT NOT NULL,
            cluster_id BIGINT NOT NULL,
            added_date TIMESTAMPTZ NOT NULL,
            similarity_score REAL,
            FOREIGN KEY (article_id) REFERENCES articles (id) ON DELETE CASCADE,
            FOREIGN KEY (cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
            UNIQUE(article_id, cluster_id)
        );

        CREATE INDEX idx_article_cluster_mappings_article_id ON article_cluster_mappings (article_id);
        CREATE INDEX idx_article_cluster_mappings_cluster_id ON article_cluster_mappings (cluster_id);

        -- Cluster merge history
        CREATE TABLE cluster_merge_history (
            id BIGSERIAL PRIMARY KEY,
            original_cluster_id BIGINT NOT NULL,
            merged_into_cluster_id BIGINT NOT NULL,
            merge_date TIMESTAMPTZ NOT NULL,
            merge_reason TEXT,
            FOREIGN KEY (original_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE,
            FOREIGN KEY (merged_into_cluster_id) REFERENCES article_clusters (id) ON DELETE CASCADE
        );

        CREATE INDEX idx_cluster_merge_original ON cluster_merge_history (original_cluster_id);
        CREATE INDEX idx_cluster_merge_destination ON cluster_merge_history (merged_into_cluster_id);

        -- Entity alias system tables
        CREATE TABLE entity_aliases (
            id BIGSERIAL PRIMARY KEY,
            entity_id BIGINT,
            canonical_name TEXT NOT NULL,
            alias_text TEXT NOT NULL,
            normalized_canonical TEXT NOT NULL,
            normalized_alias TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            source TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 1.0,
            created_at TIMESTAMPTZ NOT NULL,
            approved_by TEXT,
            approved_at TIMESTAMPTZ,
            status TEXT NOT NULL DEFAULT 'APPROVED',
            UNIQUE (normalized_canonical, normalized_alias, entity_type),
            FOREIGN KEY (entity_id) REFERENCES entities (id) ON DELETE SET NULL
        );

        CREATE INDEX idx_entity_aliases_canonical ON entity_aliases(normalized_canonical, entity_type);
        CREATE INDEX idx_entity_aliases_alias ON entity_aliases(normalized_alias, entity_type);
        CREATE INDEX idx_entity_aliases_status ON entity_aliases(status);
        CREATE INDEX idx_entity_aliases_source ON entity_aliases(source);

        -- Negative match table for explicitly rejected pairs
        CREATE TABLE entity_negative_matches (
            id BIGSERIAL PRIMARY KEY,
            entity_id1 BIGINT,
            entity_id2 BIGINT,
            normalized_name1 TEXT NOT NULL,
            normalized_name2 TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            rejected_by TEXT NOT NULL,
            rejected_at TIMESTAMPTZ NOT NULL,
            rejection_reason TEXT,
            persistence_level INTEGER NOT NULL DEFAULT 1,
            UNIQUE (normalized_name1, normalized_name2, entity_type),
            FOREIGN KEY (entity_id1) REFERENCES entities (id) ON DELETE SET NULL,
            FOREIGN KEY (entity_id2) REFERENCES entities (id) ON DELETE SET NULL
        );

        CREATE INDEX idx_negative_matches_names ON entity_negative_matches(normalized_name1, normalized_name2);
        CREATE INDEX idx_negative_matches_type ON entity_negative_matches(entity_type);

        -- Alias pattern performance tracking
        CREATE TABLE alias_pattern_stats (
            pattern_id TEXT PRIMARY KEY,
            pattern_type TEXT NOT NULL,
            total_suggestions INTEGER NOT NULL DEFAULT 0,
            approved_count INTEGER NOT NULL DEFAULT 0,
            rejected_count INTEGER NOT NULL DEFAULT 0,
            last_used_at TIMESTAMPTZ,
            enabled BOOLEAN NOT NULL DEFAULT TRUE
        );

        -- For batch review in admin interface
        CREATE TABLE alias_review_batches (
            id BIGSERIAL PRIMARY KEY,
            created_at TIMESTAMPTZ NOT NULL,
            admin_id TEXT,
            status TEXT NOT NULL DEFAULT 'OPEN',
            total_count INTEGER NOT NULL DEFAULT 0,
            processed_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE alias_review_items (
            id BIGSERIAL PRIMARY KEY,
            batch_id BIGINT NOT NULL,
            alias_id BIGINT NOT NULL,
            decision TEXT,
            decided_at TIMESTAMPTZ,
            FOREIGN KEY (batch_id) REFERENCES alias_review_batches(id),
            FOREIGN KEY (alias_id) REFERENCES entity_aliases(id)
        );

        -- Cache statistics for optimization
        CREATE TABLE alias_cache_stats (
            normalized_name TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            hit_count INTEGER NOT NULL DEFAULT 0,
            last_accessed TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (normalized_name, entity_type)
        );

        -- Queue tables
        CREATE TABLE rss_queue (
            id BIGSERIAL PRIMARY KEY,
            url TEXT NOT NULL,
            normalized_url TEXT NOT NULL UNIQUE,
            title TEXT,
            seen_at TIMESTAMPTZ NOT NULL,
            pub_date TIMESTAMPTZ
        );

        CREATE UNIQUE INDEX idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url);
        CREATE UNIQUE INDEX idx_pub_date_normalized_url ON rss_queue (pub_date, normalized_url);

        CREATE TABLE matched_topics_queue (
            id BIGSERIAL PRIMARY KEY,
            article_text TEXT NOT NULL,
            article_html TEXT NOT NULL,
            article_url TEXT NOT NULL UNIQUE,
            article_title TEXT NOT NULL,
            topic_matched TEXT NOT NULL,
            article_hash TEXT NOT NULL,
            title_domain_hash TEXT NOT NULL,
            timestamp TIMESTAMPTZ NOT NULL,
            pub_date TIMESTAMPTZ
        );

        CREATE UNIQUE INDEX idx_matched_topics_article_url ON matched_topics_queue (article_url);

        CREATE TABLE life_safety_queue (
            id BIGSERIAL PRIMARY KEY,
            article_url TEXT NOT NULL UNIQUE,
            article_title TEXT NOT NULL,
            article_text TEXT NOT NULL,
            article_html TEXT NOT NULL,
            article_hash TEXT NOT NULL,
            title_domain_hash TEXT NOT NULL,
            threat TEXT,
            timestamp TIMESTAMPTZ NOT NULL,
            pub_date TIMESTAMPTZ
        );

        CREATE UNIQUE INDEX idx_life_safety_article_url ON life_safety_queue (article_url);

        -- Device management tables
        CREATE TABLE devices (
            id BIGSERIAL PRIMARY KEY,
            device_id TEXT NOT NULL UNIQUE
        );

        CREATE INDEX idx_devices_device_id ON devices (device_id);

        CREATE TABLE device_subscriptions (
            id BIGSERIAL PRIMARY KEY,
            device_id BIGINT NOT NULL,
            topic TEXT NOT NULL,
            priority TEXT,
            FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
            UNIQUE(device_id, topic)
        );

        CREATE INDEX idx_topic_device_id ON device_subscriptions (topic, device_id);
        CREATE INDEX idx_device_subscriptions_device_id_topic ON device_subscriptions (device_id, topic);

        CREATE TABLE ip_logs (
            id BIGSERIAL PRIMARY KEY,
            device_id BIGINT NOT NULL,
            ip_address TEXT NOT NULL,
            first_seen BIGINT NOT NULL,
            last_seen BIGINT NOT NULL,
            FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
            UNIQUE (device_id, ip_address)
        );

        CREATE INDEX idx_ip_logs_device_id ON ip_logs (device_id);
        CREATE INDEX idx_ip_logs_ip_address ON ip_logs (ip_address);

        -- Alert system tables for LLM endpoint monitoring
        CREATE TABLE endpoint_timeout_events (
            id BIGSERIAL PRIMARY KEY,
            endpoint_url TEXT NOT NULL,
            model_name TEXT NOT NULL,
            worker_id TEXT NOT NULL,
            worker_type TEXT NOT NULL,
            timeout_type TEXT NOT NULL,
            occurred_at TIMESTAMPTZ NOT NULL
        );

        CREATE INDEX idx_timeout_events_endpoint_model ON endpoint_timeout_events (endpoint_url, model_name);
        CREATE INDEX idx_timeout_events_occurred_at ON endpoint_timeout_events (occurred_at);
        CREATE INDEX idx_timeout_events_worker ON endpoint_timeout_events (worker_id, worker_type);

        CREATE TABLE endpoint_alerts (
            id BIGSERIAL PRIMARY KEY,
            endpoint_url TEXT NOT NULL,
            model_name TEXT NOT NULL,
            alert_type TEXT NOT NULL,
            first_occurrence TIMESTAMPTZ NOT NULL,
            last_occurrence TIMESTAMPTZ NOT NULL,
            last_alert_sent TIMESTAMPTZ,
            occurrence_count INTEGER NOT NULL DEFAULT 1,
            consecutive_failures INTEGER NOT NULL DEFAULT 1,
            is_resolved BOOLEAN NOT NULL DEFAULT FALSE,
            resolved_at TIMESTAMPTZ
        );

        CREATE INDEX idx_endpoint_alerts_endpoint_model ON endpoint_alerts (endpoint_url, model_name);
        CREATE INDEX idx_endpoint_alerts_type_resolved ON endpoint_alerts (alert_type, is_resolved);
        CREATE INDEX idx_endpoint_alerts_occurrence ON endpoint_alerts (last_occurrence);
    "#;

    sqlx::query(schema).execute(pool).await?;
    println!("✅ Complete PostgreSQL schema created (21 tables)");

    Ok(())
}

async fn migrate_data_direct_mapping(pool: &Pool<Postgres>, debug_mode: bool) -> Result<()> {
    println!("📥 Migrating data with direct column mapping...");

    // Step 1: Get all table names that exist in SQLite
    println!("  🔍 Discovering tables in SQLite database...");
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
        "  📋 Found {} tables to migrate: {:?}",
        tables.len(),
        tables
    );

    if tables.is_empty() {
        println!("  ⚠️  No tables found in SQLite database");
        return Ok(());
    }

    // Step 2: Use SQLite .dump to export all data with proper formatting
    println!("  📤 Extracting all data from SQLite...");
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
            "  📊 Extracted dump in {:.1}s",
            start_time.elapsed().as_secs_f64()
        );
    }

    // Step 3: Transform SQL for PostgreSQL compatibility
    println!("  🔄 Transforming for PostgreSQL compatibility...");
    let transform_start = Instant::now();

    let postgres_sql = sqlite_dump
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();

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
                return None;
            }

            // Process INSERT statements
            if trimmed.starts_with("INSERT INTO") {
                let mut result = line.to_string();

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

                Some(result)
            } else {
                Some(line.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let insert_count = postgres_sql
        .lines()
        .filter(|line| line.trim().starts_with("INSERT INTO"))
        .count();

    if debug_mode {
        println!(
            "  ⏱️  Transformation: {:.1}s",
            transform_start.elapsed().as_secs_f64()
        );
        println!("  📊 Generated {} INSERT statements", insert_count);
    }

    // Step 4: Import to PostgreSQL
    println!("  📥 Importing to PostgreSQL...");
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
        println!("  ⏱️  Import: {:.1}s", import_start.elapsed().as_secs_f64());
    }

    // Cleanup
    let _ = fs::remove_file(temp_file);

    // Step 5: Reset sequences for all tables
    println!("  🔢 Resetting PostgreSQL sequences...");
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
        "✅ Data migration completed: {} tables, {} INSERT statements",
        tables.len(),
        insert_count
    );
    Ok(())
}

async fn validate_simple_migration(pool: &Pool<Postgres>) -> Result<()> {
    println!("✅ Validating migration...");

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

    println!("  📊 SQLite articles: {}", sqlite_count);
    println!("  📊 PostgreSQL articles: {}", pg_count);

    if pg_count == sqlite_count {
        println!("  ✅ Record counts match");
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

    println!("  📋 Sample records:");
    for (id, url) in sample {
        println!("    {}: {}", id, &url[..url.len().min(50)]);
    }

    println!("✅ Migration validation completed");
    Ok(())
}
