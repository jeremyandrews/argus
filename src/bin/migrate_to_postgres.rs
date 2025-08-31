//! PostgreSQL Migration Tool - Clean & Reliable Implementation
//!
//! This tool provides a direct migration path from SQLite to PostgreSQL with:
//! 1. Direct table-by-table migration using sqlx
//! 2. Batch processing for large datasets (800K+ records)
//! 3. --partial flag support for incremental migration
//! 4. Built-in validation and progress tracking
//! 5. Simple, reliable approach following KISS principle

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions, Column, PgPool, Row, SqlitePool};
use std::collections::HashMap;
use std::env;
use tracing::{debug, info, warn};

#[derive(Parser, Debug)]
#[command(name = "migrate_to_postgres")]
#[command(about = "Migrate Argus database from SQLite to PostgreSQL")]
struct Args {
    /// PostgreSQL connection string (or use DATABASE_URL env var)
    #[arg(long)]
    postgres_url: Option<String>,

    /// SQLite database path
    #[arg(long, default_value = "argus.db")]
    sqlite_path: String,

    /// Perform partial migration using high-water marks
    #[arg(long)]
    partial: bool,

    /// Dry run - show what would be migrated
    #[arg(long)]
    dry_run: bool,

    /// Skip schema creation
    #[arg(long)]
    skip_schema: bool,

    /// Specific tables to migrate (comma-separated)
    #[arg(long)]
    tables: Option<String>,

    /// Batch size for processing
    #[arg(long, default_value = "1000")]
    batch_size: usize,

    /// Validate data after migration
    #[arg(long)]
    validate: bool,
}

#[derive(Debug, Clone)]
struct MigrationStats {
    table_name: String,
    total_records: i64,
    migrated_records: i64,
    last_migrated_id: Option<i64>,
}

struct MigrationContext {
    sqlite_pool: SqlitePool,
    pg_pool: PgPool,
    args: Args,
    stats: HashMap<String, MigrationStats>,
}

impl MigrationContext {
    async fn new(args: Args) -> Result<Self> {
        // Setup SQLite connection
        let sqlite_url = format!("sqlite:{}", args.sqlite_path);
        let sqlite_pool = SqlitePoolOptions::new()
            .max_connections(1) // SQLite works best with single connection
            .connect(&sqlite_url)
            .await
            .context("Failed to connect to SQLite")?;

        // Setup PostgreSQL connection
        let postgres_url = args
            .postgres_url
            .clone()
            .or_else(|| env::var("DATABASE_URL").ok())
            .context("PostgreSQL connection string required (--postgres-url or DATABASE_URL)")?;

        let pg_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&postgres_url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        // Test connections
        sqlx::query("SELECT 1")
            .fetch_one(&sqlite_pool)
            .await
            .context("SQLite connection test failed")?;
        sqlx::query("SELECT 1")
            .fetch_one(&pg_pool)
            .await
            .context("PostgreSQL connection test failed")?;

        Ok(Self {
            sqlite_pool,
            pg_pool,
            args,
            stats: HashMap::new(),
        })
    }

    async fn run(&mut self) -> Result<()> {
        info!("Starting PostgreSQL migration");
        info!("SQLite: {}", self.args.sqlite_path);
        info!("Partial migration: {}", self.args.partial);
        info!("Dry run: {}", self.args.dry_run);

        // Create schema if needed
        if !self.args.skip_schema && !self.args.dry_run {
            self.create_schema().await?;
        }

        // Get tables to migrate
        let tables = self.get_migration_tables().await?;
        info!("Tables to migrate: {:?}", tables);

        // Migrate each table
        for table in &tables {
            self.migrate_table(table).await?;
        }

        // Validate if requested
        if self.args.validate && !self.args.dry_run {
            self.validate_migration(&tables).await?;
        }

        self.print_summary();
        Ok(())
    }

    async fn create_schema(&mut self) -> Result<()> {
        info!("Creating PostgreSQL schema");

        // Drop existing tables (in dependency order)
        let drop_tables = vec![
            "device_subscriptions",
            "devices",
            "matched_topics_queue",
            "rss_queue",
            "articles",
            "migration_tracking",
        ];

        for table in drop_tables {
            sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", table))
                .execute(&self.pg_pool)
                .await?;
        }

        // Create tables one by one with individual error handling
        info!("Creating migration_tracking table");
        sqlx::query(
            r#"
            CREATE TABLE migration_tracking (
                table_name TEXT PRIMARY KEY,
                last_migrated_id BIGINT,
                migrated_count BIGINT DEFAULT 0,
                last_updated TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create migration_tracking table")?;

        info!("Creating articles table");
        sqlx::query(
            r#"
            CREATE TABLE articles (
                id BIGSERIAL PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                seen_at TEXT NOT NULL,
                is_relevant BOOLEAN NOT NULL,
                category TEXT,
                analysis TEXT,
                normalized_url TEXT,
                hash TEXT,
                tiny_summary TEXT,
                title_domain_hash TEXT,
                r2_url TEXT,
                pub_date TEXT,
                event_date TEXT,
                cluster_id INTEGER,
                title TEXT,
                json_data TEXT,
                quality REAL,
                source TEXT
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create articles table")?;

        info!("Creating rss_queue table");
        sqlx::query(
            r#"
            CREATE TABLE rss_queue (
                id BIGSERIAL PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT,
                seen_at TEXT NOT NULL,
                normalized_url TEXT,
                pub_date TEXT
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create rss_queue table")?;

        info!("Creating matched_topics_queue table");
        sqlx::query(
            r#"
            CREATE TABLE matched_topics_queue (
                id BIGSERIAL PRIMARY KEY,
                article_text TEXT NOT NULL,
                article_html TEXT NOT NULL,
                article_url TEXT NOT NULL UNIQUE,
                article_title TEXT NOT NULL,
                topic_matched TEXT NOT NULL,
                article_hash TEXT NOT NULL,
                title_domain_hash TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                pub_date TEXT
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create matched_topics_queue table")?;

        info!("Creating devices table");
        sqlx::query(
            r#"
            CREATE TABLE devices (
                id BIGSERIAL PRIMARY KEY,
                device_id TEXT NOT NULL UNIQUE
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create devices table")?;

        info!("Creating device_subscriptions table");
        sqlx::query(
            r#"
            CREATE TABLE device_subscriptions (
                id BIGSERIAL PRIMARY KEY,
                device_id INTEGER NOT NULL,
                topic TEXT NOT NULL,
                priority TEXT,
                FOREIGN KEY (device_id) REFERENCES devices (id) ON DELETE CASCADE,
                UNIQUE(device_id, topic)
            )
        "#,
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create device_subscriptions table")?;

        // Verify all tables were created
        let tables_check = sqlx::query(
            "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
        )
        .fetch_all(&self.pg_pool)
        .await?;
        let created_tables: Vec<String> = tables_check
            .iter()
            .map(|row| row.get::<String, _>("tablename"))
            .collect();
        info!("Created tables: {:?}", created_tables);

        // Verify critical tables exist before creating indexes
        let required_tables = vec![
            "articles",
            "rss_queue",
            "matched_topics_queue",
            "devices",
            "device_subscriptions",
        ];
        for table in &required_tables {
            if !created_tables.contains(&table.to_string()) {
                return Err(anyhow::anyhow!(
                    "Required table '{}' was not created",
                    table
                ));
            }
        }

        // Now create indexes - one by one with individual error handling
        info!("Creating indexes on articles table");
        sqlx::query("CREATE INDEX idx_relevant_category ON articles (is_relevant, category)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_relevant_category index")?;

        sqlx::query("CREATE INDEX idx_articles_normalized_url ON articles (normalized_url)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_articles_normalized_url index")?;

        sqlx::query("CREATE INDEX idx_r2_url ON articles (r2_url)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_r2_url index")?;

        sqlx::query("CREATE INDEX idx_seen_at_r2_url ON articles (seen_at, r2_url)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_seen_at_r2_url index")?;

        sqlx::query(
            "CREATE INDEX idx_seen_at_category_r2_url ON articles (seen_at, category, r2_url)",
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create idx_seen_at_category_r2_url index")?;

        info!("Creating indexes on rss_queue table");
        sqlx::query("CREATE INDEX idx_rss_queue_normalized_url ON rss_queue (normalized_url)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_rss_queue_normalized_url index")?;

        sqlx::query("CREATE INDEX idx_seen_at_url ON rss_queue (seen_at, url)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_seen_at_url index")?;

        sqlx::query(
            "CREATE INDEX idx_seen_at_normalized_url ON rss_queue (seen_at, normalized_url)",
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create idx_seen_at_normalized_url index")?;

        info!("Creating indexes on other tables");
        sqlx::query(
            "CREATE INDEX idx_matched_topics_article_url ON matched_topics_queue (article_url)",
        )
        .execute(&self.pg_pool)
        .await
        .context("Failed to create idx_matched_topics_article_url index")?;

        sqlx::query("CREATE INDEX idx_devices_device_id ON devices (device_id)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_devices_device_id index")?;

        sqlx::query("CREATE INDEX idx_topic_device_id_priority ON device_subscriptions (topic, device_id, priority)")
            .execute(&self.pg_pool).await
            .context("Failed to create idx_topic_device_id_priority index")?;

        sqlx::query("CREATE INDEX idx_topic_device_id ON device_subscriptions (topic, device_id)")
            .execute(&self.pg_pool)
            .await
            .context("Failed to create idx_topic_device_id index")?;

        sqlx::query("CREATE INDEX idx_device_subscriptions_device_id_topic ON device_subscriptions (device_id, topic)")
            .execute(&self.pg_pool).await
            .context("Failed to create idx_device_subscriptions_device_id_topic index")?;

        info!("PostgreSQL schema created successfully with all tables and indexes");
        Ok(())
    }

    async fn get_migration_tables(&self) -> Result<Vec<String>> {
        if let Some(table_list) = &self.args.tables {
            return Ok(table_list
                .split(',')
                .map(|s| s.trim().to_string())
                .collect());
        }

        // Get all tables from SQLite
        let rows = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name != 'sqlite_sequence' ORDER BY name")
            .fetch_all(&self.sqlite_pool).await?;

        let mut tables = Vec::new();
        for row in rows {
            let table_name: String = row.get(0);
            tables.push(table_name);
        }

        Ok(tables)
    }

    async fn migrate_table(&mut self, table_name: &str) -> Result<()> {
        info!("Migrating table: {}", table_name);

        // Check if the table exists in PostgreSQL
        let pg_table_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public'"
        )
        .bind(table_name)
        .fetch_one(&self.pg_pool)
        .await?;

        if pg_table_exists == 0 {
            warn!(
                "Table {} does not exist in PostgreSQL schema, skipping migration",
                table_name
            );
            return Ok(());
        }

        // Get total count
        let total_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table_name))
            .fetch_one(&self.sqlite_pool)
            .await?;

        if total_count == 0 {
            info!("Table {} is empty, skipping", table_name);
            return Ok(());
        }

        info!("Table {} has {} records", table_name, total_count);

        // Initialize stats
        let mut stats = MigrationStats {
            table_name: table_name.to_string(),
            total_records: total_count,
            migrated_records: 0,
            last_migrated_id: None,
        };

        // Get starting point for partial migration
        let mut start_id = 0;
        if self.args.partial {
            start_id = self.get_last_migrated_id(table_name).await?;
            if start_id > 0 {
                info!("Partial migration starting from ID {}", start_id);
            }
        }

        // Migrate in batches
        let mut current_id = start_id;
        let mut batch_num = 0;

        loop {
            batch_num += 1;
            let batch_count = self
                .migrate_batch(table_name, current_id, &mut stats)
                .await?;

            if batch_count == 0 {
                break; // No more records
            }

            // Update progress
            let progress = (stats.migrated_records as f64 / total_count as f64) * 100.0;
            info!(
                "Progress {}: {:.1}% ({}/{}) - batch {}",
                table_name, progress, stats.migrated_records, total_count, batch_num
            );

            // Update high-water mark
            if !self.args.dry_run {
                self.update_migration_tracking(
                    table_name,
                    stats.last_migrated_id,
                    stats.migrated_records,
                )
                .await?;
            }

            // Safely update current_id, handling tables without ID columns
            if let Some(last_id) = stats.last_migrated_id {
                current_id = last_id + 1;
            } else {
                // For tables without ID column, we just continue since we use OFFSET
                break;
            }
        }

        info!(
            "Completed migration of {}: {} records",
            table_name, stats.migrated_records
        );

        // Reset PostgreSQL sequence for tables with ID columns
        if let Some(last_id) = stats.last_migrated_id {
            if !self.args.dry_run {
                let seq_name = format!("{}_id_seq", table_name);
                let reset_sql = format!("SELECT setval('{}', {})", seq_name, last_id);
                let _ = sqlx::query(&reset_sql).execute(&self.pg_pool).await; // Ignore errors for tables without sequences
            }
        }

        self.stats.insert(table_name.to_string(), stats);

        Ok(())
    }

    async fn table_has_id_column(&self, table_name: &str) -> Result<bool> {
        let query = "SELECT COUNT(*) as count FROM pragma_table_info(?) WHERE name = 'id'";
        let count: i64 = sqlx::query(query)
            .bind(table_name)
            .fetch_one(&self.sqlite_pool)
            .await?
            .try_get("count")?;
        Ok(count > 0)
    }

    async fn migrate_batch(
        &mut self,
        table_name: &str,
        start_id: i64,
        stats: &mut MigrationStats,
    ) -> Result<usize> {
        // Check if table has an ID column
        let has_id_column = self.table_has_id_column(table_name).await?;

        let rows = if has_id_column {
            // Get batch of records from SQLite using ID
            let query = format!(
                "SELECT * FROM {} WHERE id > ? ORDER BY id LIMIT ?",
                table_name
            );
            sqlx::query(&query)
                .bind(start_id)
                .bind(self.args.batch_size as i64)
                .fetch_all(&self.sqlite_pool)
                .await?
        } else {
            // For tables without ID, use LIMIT/OFFSET
            let offset = stats.migrated_records;
            let query = format!("SELECT * FROM {} LIMIT ? OFFSET ?", table_name);
            sqlx::query(&query)
                .bind(self.args.batch_size as i64)
                .bind(offset)
                .fetch_all(&self.sqlite_pool)
                .await?
        };

        if rows.is_empty() {
            return Ok(0);
        }

        if self.args.dry_run {
            if has_id_column {
                if let Some(last_row) = rows.last() {
                    if let Ok(last_id) = last_row.try_get::<i64, _>("id") {
                        stats.last_migrated_id = Some(last_id);
                    }
                }
            }
            stats.migrated_records += rows.len() as i64;
            debug!(
                "[DRY RUN] Would migrate {} records for table {}",
                rows.len(),
                table_name
            );
            return Ok(rows.len());
        }

        // Prepare INSERT statement for PostgreSQL
        let (insert_sql, column_names) = self.build_insert_statement(table_name, &rows[0]).await?;

        // Insert batch into PostgreSQL
        for row in &rows {
            // Build query with parameters
            let mut query = sqlx::query(&insert_sql);

            for column_name in &column_names {
                if column_name == "id" {
                    continue; // Skip ID column, let PostgreSQL auto-generate
                }

                // Handle different data types and bind to query
                match column_name.as_str() {
                    "is_relevant" => {
                        // Convert SQLite boolean (0/1) to PostgreSQL boolean
                        let val: Option<i64> = row.try_get(column_name.as_str()).unwrap_or(None);
                        query = query.bind(val.map(|v| v != 0));
                    }
                    "quality" => {
                        let val: Option<f64> = row.try_get(column_name.as_str()).unwrap_or(None);
                        query = query.bind(val);
                    }
                    "cluster_id" => {
                        let val: Option<i64> = row.try_get(column_name.as_str()).unwrap_or(None);
                        query = query.bind(val.map(|v| v as i32));
                    }
                    _ => {
                        // Handle as text
                        let val: Option<String> = row.try_get(column_name.as_str()).unwrap_or(None);
                        query = query.bind(val);
                    }
                }
            }

            let result = query.execute(&self.pg_pool).await;
            if let Err(e) = result {
                warn!("Failed to insert record into {}: {}", table_name, e);
                continue; // Skip this record and continue
            }

            stats.migrated_records += 1;
        }

        // Update last migrated ID safely
        if let Some(last_row) = rows.last() {
            if has_id_column {
                if let Ok(last_id) = last_row.try_get::<i64, _>("id") {
                    stats.last_migrated_id = Some(last_id);
                }
            }
        }

        Ok(rows.len())
    }

    async fn build_insert_statement(
        &self,
        table_name: &str,
        sample_row: &sqlx::sqlite::SqliteRow,
    ) -> Result<(String, Vec<String>)> {
        // Get column names from the sample row
        let mut column_names = Vec::new();
        for i in 0..sample_row.len() {
            column_names.push(sample_row.column(i).name().to_string());
        }

        // Filter out the ID column (PostgreSQL will auto-generate)
        let insert_columns: Vec<String> = column_names
            .iter()
            .filter(|&name| name != "id")
            .cloned()
            .collect();

        // Build parameterized INSERT statement
        let placeholders: Vec<String> = (1..=insert_columns.len())
            .map(|i| format!("${}", i))
            .collect();

        let conflict_resolution = match table_name {
            "articles" => "ON CONFLICT (url) DO NOTHING",
            "rss_queue" => "ON CONFLICT (url) DO NOTHING",
            "matched_topics_queue" => "ON CONFLICT (article_url) DO NOTHING",
            "devices" => "ON CONFLICT (device_id) DO NOTHING",
            "device_subscriptions" => "ON CONFLICT (device_id, topic) DO NOTHING",
            _ => "",
        };

        let insert_sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) {}",
            table_name,
            insert_columns.join(", "),
            placeholders.join(", "),
            conflict_resolution
        );

        Ok((insert_sql, column_names))
    }

    async fn get_last_migrated_id(&self, table_name: &str) -> Result<i64> {
        let result: Option<i64> = sqlx::query_scalar(
            "SELECT last_migrated_id FROM migration_tracking WHERE table_name = $1",
        )
        .bind(table_name)
        .fetch_optional(&self.pg_pool)
        .await?;

        Ok(result.unwrap_or(0))
    }

    async fn update_migration_tracking(
        &self,
        table_name: &str,
        last_id: Option<i64>,
        count: i64,
    ) -> Result<()> {
        if let Some(last_id) = last_id {
            sqlx::query(
                "INSERT INTO migration_tracking (table_name, last_migrated_id, migrated_count, last_updated) 
                 VALUES ($1, $2, $3, CURRENT_TIMESTAMP) 
                 ON CONFLICT (table_name) 
                 DO UPDATE SET last_migrated_id = $2, migrated_count = $3, last_updated = CURRENT_TIMESTAMP"
            )
            .bind(table_name)
            .bind(last_id)
            .bind(count)
            .execute(&self.pg_pool).await?;
        }
        Ok(())
    }

    async fn validate_migration(&self, tables: &[String]) -> Result<()> {
        info!("Validating migration...");

        for table_name in tables {
            // Get counts from both databases
            let sqlite_count: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table_name))
                    .fetch_one(&self.sqlite_pool)
                    .await
                    .unwrap_or(0);

            let pg_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table_name))
                .fetch_one(&self.pg_pool)
                .await
                .unwrap_or(0);

            if sqlite_count == pg_count {
                info!("✅ {}: {} records (matches)", table_name, pg_count);
            } else {
                warn!(
                    "❌ {}: SQLite={}, PostgreSQL={} (MISMATCH)",
                    table_name, sqlite_count, pg_count
                );
            }
        }

        Ok(())
    }

    fn print_summary(&self) {
        info!("=== Migration Summary ===");
        for (_table, stats) in &self.stats {
            let success_rate = if stats.total_records > 0 {
                (stats.migrated_records as f64 / stats.total_records as f64) * 100.0
            } else {
                100.0
            };

            info!(
                "{}: {}/{} migrated ({:.1}%)",
                stats.table_name, stats.migrated_records, stats.total_records, success_rate
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("migrate_to_postgres=info")
        .init();

    let args = Args::parse();

    let mut migration = MigrationContext::new(args)
        .await
        .context("Failed to initialize migration context")?;

    migration.run().await.context("Migration failed")?;

    info!("Migration completed successfully!");
    Ok(())
}
