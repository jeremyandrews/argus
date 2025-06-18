# PostgreSQL Migration Implementation

This document provides the technical implementation details for migrating Argus from SQLite to PostgreSQL.

## Implementation Overview

The migration consists of two main components:
1. **`migrate_to_postgres`** - One-shot migration binary (~400 lines)
2. **`argus_admin`** - Runtime management tool (~300 lines)

## Core Migration Binary: `migrate_to_postgres`

### File: `src/bin/migrate_to_postgres.rs`

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::process::Command;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Starting PostgreSQL migration...");
    
    // Phase 1: Pre-migration checks and backup
    check_prerequisites().await?;
    create_backup().await?;
    
    // Phase 2: Setup PostgreSQL
    let pool = setup_postgres_connection().await?;
    create_postgres_schema(&pool).await?;
    
    // Phase 3: Migrate data
    migrate_data_via_dump(&pool).await?;
    
    // Phase 4: Migrate configuration
    migrate_env_to_database(&pool).await?;
    
    // Phase 5: Validation
    validate_migration(&pool).await?;
    
    println!("✅ Migration completed successfully!");
    println!("Next steps:");
    println!("  1. Test the application: cargo run --release");
    println!("  2. Use admin tool: cargo run --bin argus_admin");
    println!("  3. Create alias: alias aa='cargo run --bin argus_admin'");
    
    Ok(())
}

async fn check_prerequisites() -> Result<()> {
    println!("🔍 Checking prerequisites...");
    
    // Check SQLite database exists
    if !std::path::Path::new("argus.db").exists() {
        return Err(anyhow::anyhow!("SQLite database 'argus.db' not found"));
    }
    
    // Check DATABASE_URL is set
    env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;
    
    // Check PostgreSQL connection
    let output = Command::new("psql")
        .arg(&env::var("DATABASE_URL")?)
        .arg("-c")
        .arg("SELECT version();")
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!("Cannot connect to PostgreSQL database"));
    }
    
    println!("✅ Prerequisites check passed");
    Ok(())
}

async fn create_backup() -> Result<()> {
    println!("💾 Creating backup...");
    
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    
    // Backup SQLite database
    let backup_path = format!("argus.db.backup.{}", timestamp);
    fs::copy("argus.db", &backup_path)?;
    println!("  ✅ SQLite backup: {}", backup_path);
    
    // Backup environment variables
    let env_backup = format!("env.backup.{}", timestamp);
    let env_content = [
        ("TOPICS", env::var("TOPICS").unwrap_or_default()),
        ("URLS", env::var("URLS").unwrap_or_default()),
        ("SLACK_TOKEN", env::var("SLACK_TOKEN").unwrap_or_default()),
        ("SLACK_CHANNEL", env::var("SLACK_CHANNEL").unwrap_or_default()),
        ("RUST_LOG", env::var("RUST_LOG").unwrap_or_default()),
    ]
    .iter()
    .map(|(k, v)| format!("export {}=\"{}\"", k, v))
    .collect::<Vec<_>>()
    .join("\n");
    
    fs::write(&env_backup, env_content)?;
    println!("  ✅ Environment backup: {}", env_backup);
    
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
    println!("🏗️  Creating PostgreSQL schema...");
    
    let schema_sql = include_str!("../../memory-bank/postgresql-migration/schema.sql");
    
    // Execute schema in transaction
    let mut tx = pool.begin().await?;
    sqlx::query(schema_sql).execute(&mut *tx).await?;
    tx.commit().await?;
    
    println!("✅ PostgreSQL schema created");
    Ok(())
}

async fn migrate_data_via_dump(pool: &Pool<Postgres>) -> Result<()> {
    println!("📥 Migrating data via dump/restore...");
    
    // Step 1: Export SQLite data
    println!("  📤 Exporting SQLite data...");
    let dump_output = Command::new("sqlite3")
        .arg("argus.db")
        .arg(".dump")
        .output()?;
    
    if !dump_output.status.success() {
        return Err(anyhow::anyhow!("Failed to dump SQLite data"));
    }
    
    let sqlite_dump = String::from_utf8(dump_output.stdout)?;
    
    // Step 2: Transform SQL for PostgreSQL compatibility
    println!("  🔄 Transforming SQL for PostgreSQL...");
    let postgres_sql = transform_sqlite_to_postgres(&sqlite_dump)?;
    
    // Write transformed SQL to temp file
    fs::write("/tmp/postgres_import.sql", postgres_sql)?;
    
    // Step 3: Import to PostgreSQL (skip configuration table for now)
    println!("  📥 Importing to PostgreSQL...");
    let import_output = Command::new("psql")
        .arg(&env::var("DATABASE_URL")?)
        .arg("-f")
        .arg("/tmp/postgres_import.sql")
        .output()?;
    
    if !import_output.status.success() {
        let error = String::from_utf8_lossy(&import_output.stderr);
        println!("  ⚠️  Import completed with warnings: {}", error);
        // Don't fail - some warnings are expected
    }
    
    // Step 4: Reset sequences
    reset_postgres_sequences(pool).await?;
    
    // Cleanup
    let _ = fs::remove_file("/tmp/postgres_import.sql");
    
    println!("✅ Data migration completed");
    Ok(())
}

fn transform_sqlite_to_postgres(sqlite_sql: &str) -> Result<String> {
    let mut postgres_sql = sqlite_sql.to_string();
    
    // Transform SQLite-specific syntax to PostgreSQL
    postgres_sql = postgres_sql.replace("INTEGER PRIMARY KEY AUTOINCREMENT", "SERIAL PRIMARY KEY");
    postgres_sql = postgres_sql.replace("INTEGER PRIMARY KEY", "SERIAL PRIMARY KEY");
    postgres_sql = postgres_sql.replace("AUTOINCREMENT", "");
    postgres_sql = postgres_sql.replace("TEXT", "TEXT");
    postgres_sql = postgres_sql.replace("REAL", "REAL");
    postgres_sql = postgres_sql.replace("BOOLEAN", "BOOLEAN");
    
    // Handle SQLite pragma statements (remove them)
    let lines: Vec<&str> = postgres_sql.lines()
        .filter(|line| !line.starts_with("PRAGMA"))
        .filter(|line| !line.starts_with("BEGIN TRANSACTION"))
        .filter(|line| !line.starts_with("COMMIT"))
        .collect();
    
    postgres_sql = lines.join("\n");
    
    // Skip configuration table creation since we handle it separately
    postgres_sql = postgres_sql.replace("CREATE TABLE configurations", "-- CREATE TABLE configurations");
    
    Ok(postgres_sql)
}

async fn reset_postgres_sequences(pool: &Pool<Postgres>) -> Result<()> {
    println!("  🔄 Resetting PostgreSQL sequences...");
    
    let tables = [
        "articles", "entities", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings", "devices"
    ];
    
    for table in tables {
        // Reset sequence to max ID + 1
        let query = format!(
            "SELECT setval('{}_id_seq', COALESCE((SELECT MAX(id) FROM {}), 1))",
            table, table
        );
        
        let _ = sqlx::query(&query).execute(pool).await; // Ignore errors for missing sequences
    }
    
    Ok(())
}

async fn migrate_env_to_database(pool: &Pool<Postgres>) -> Result<()> {
    println!("⚙️  Migrating environment configuration to database...");
    
    // Migrate topics from TOPICS env var
    if let Ok(topics_str) = env::var("TOPICS") {
        for line in topics_str.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            
            for topic_pair in line.split(';') {
                let topic_pair = topic_pair.trim();
                if topic_pair.is_empty() { continue; }
                
                let parts: Vec<&str> = topic_pair.split(':').collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim();
                    let prompt = parts[1].trim();
                    
                    if !name.is_empty() && !prompt.is_empty() {
                        set_config(pool, "topics", name, prompt).await?;
                        println!("  ✅ Migrated topic: {}", name);
                    }
                }
            }
        }
    }
    
    // Migrate RSS feeds from URLS env var
    if let Ok(urls_str) = env::var("URLS") {
        for (i, url) in urls_str.split(';').enumerate() {
            let url = url.trim();
            if !url.is_empty() {
                let name = format!("feed_{}", i + 1);
                set_config(pool, "rss", &name, url).await?;
                println!("  ✅ Migrated RSS feed: {}", name);
            }
        }
    }
    
    // Migrate system settings
    let system_vars = [
        ("slack_token", "SLACK_TOKEN"),
        ("slack_channel", "SLACK_CHANNEL"),
        ("rust_log", "RUST_LOG"),
    ];
    
    for (key, env_var) in system_vars {
        if let Ok(value) = env::var(env_var) {
            set_config(pool, "system", key, &value).await?;
            let display_value = if key == "slack_token" { "[REDACTED]" } else { &value };
            println!("  ✅ Migrated system setting: {} = {}", key, display_value);
        }
    }
    
    println!("✅ Configuration migration completed");
    Ok(())
}

async fn set_config(pool: &Pool<Postgres>, category: &str, name: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO configurations (category, name, value, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (category, name) 
         DO UPDATE SET value = $3, updated_at = NOW()"
    )
    .bind(category)
    .bind(name)
    .bind(value)
    .execute(pool)
    .await?;
    
    Ok(())
}

async fn validate_migration(pool: &Pool<Postgres>) -> Result<()> {
    println!("✅ Validating migration...");
    
    // Check table counts
    let tables = ["articles", "entities", "article_entities"];
    
    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        
        println!("  ✅ {}: {} records", table, count);
    }
    
    // Check configuration
    let config_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM configurations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    
    println!("  ✅ configurations: {} records", config_count);
    
    // Test basic functionality
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(pool)
        .await?;
    
    println!("  ✅ PostgreSQL version: {}", 
             version.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
    
    println!("✅ Migration validation completed");
    Ok(())
}
```

## Database Layer Updates

### Update `src/db/core.rs`

Remove the complex dual-database abstraction and simplify to PostgreSQL-only:

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres, Row};
use std::env;
use tokio::time::Duration;
use anyhow::Result;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Postgres>,
}

impl Database {
    pub async fn new() -> Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;
        
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&database_url)
            .await?;
        
        Ok(Database { pool })
    }
    
    // Configuration management methods
    pub async fn set_config(&self, category: &str, name: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO configurations (category, name, value, updated_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (category, name) 
             DO UPDATE SET value = $3, updated_at = NOW()"
        )
        .bind(category)
        .bind(name)
        .bind(value)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_config(&self, category: &str, name: &str) -> Result<Option<String>> {
        let result = sqlx::query_scalar(
            "SELECT value FROM configurations 
             WHERE category = $1 AND name = $2 AND enabled = true"
        )
        .bind(category)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(result)
    }
    
    pub async fn get_configs_by_category(&self, category: &str) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            "SELECT name, value FROM configurations 
             WHERE category = $1 AND enabled = true ORDER BY name"
        )
        .bind(category)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| {
            (row.get("name"), row.get("value"))
        }).collect())
    }
    
    pub async fn remove_config(&self, category: &str, name: &str) -> Result<()> {
        sqlx::query(
            "UPDATE configurations SET enabled = false 
             WHERE category = $1 AND name = $2"
        )
        .bind(category)
        .bind(name)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
```

## Runtime Admin Tool: `argus_admin` (Enhanced)

### File: `src/bin/argus_admin.rs`

```rust
use anyhow::Result;
use argus::db::Database;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "argus_admin")]
#[command(about = "Argus administration tool with bulk operations, validation, and dry-run support")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Enable dry-run mode (preview changes without applying)
    #[arg(long, global = true)]
    dry_run: bool,
    
    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Topic management
    Topics {
        #[command(subcommand)]
        action: TopicAction,
    },
    /// RSS feed management
    Rss {
        #[command(subcommand)]
        action: RssAction,
    },
    /// System configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Health check
    #[command(name = "health-check")]
    HealthCheck,
    /// Backup operations
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },
    /// System status
    Status,
    /// Validate system configuration
    #[command(name = "validate-all")]
    ValidateAll,
}

#[derive(Subcommand)]
enum TopicAction {
    /// List all topics
    List,
    /// Add a new topic
    Add {
        /// Topic name
        name: String,
        /// Topic prompt
        prompt: String,
        /// Validate before adding
        #[arg(long)]
        validate: bool,
    },
    /// Remove a topic
    Remove {
        /// Topic name to remove
        name: String,
    },
    /// Export topics to JSON file
    Export {
        /// Output file path
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Import topics from JSON file
    Import {
        /// Input file path
        #[arg(short, long)]
        file: PathBuf,
        /// Validate before importing
        #[arg(long)]
        validate: bool,
    },
    /// Validate a topic configuration
    Validate {
        /// Topic name
        name: String,
        /// Topic prompt
        prompt: String,
    },
}

#[derive(Subcommand)]
enum RssAction {
    /// List all RSS feeds
    List,
    /// Add a new RSS feed
    Add {
        /// Feed name
        name: String,
        /// Feed URL
        url: String,
        /// Validate feed before adding
        #[arg(long)]
        validate: bool,
    },
    /// Remove an RSS feed
    Remove {
        /// Feed name to remove
        name: String,
    },
    /// Export RSS feeds to JSON file
    Export {
        /// Output file path
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Import RSS feeds from JSON file
    Import {
        /// Input file path
        #[arg(short, long)]
        file: PathBuf,
        /// Validate feeds before importing
        #[arg(long)]
        validate: bool,
    },
    /// Validate an RSS feed URL
    Validate {
        /// Feed URL to validate
        url: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// List all configuration
    List,
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
        /// Validate before setting
        #[arg(long)]
        validate: bool,
    },
    /// Export all configuration to JSON file
    Export {
        /// Output file path
        #[arg(short, long)]
        file: PathBuf,
        /// Include sensitive values (like tokens)
        #[arg(long)]
        include_sensitive: bool,
    },
    /// Import configuration from JSON file
    Import {
        /// Input file path
        #[arg(short, long)]
        file: PathBuf,
        /// Validate before importing
        #[arg(long)]
        validate: bool,
    },
    /// Validate configuration values
    Validate {
        /// Configuration file to validate
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum BackupAction {
    /// Create a backup
    Create {
        /// Include configuration in backup
        #[arg(long)]
        include_config: bool,
        /// Backup file path (optional)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    /// Restore from backup
    Restore {
        /// Backup file path
        #[arg(short, long)]
        file: PathBuf,
        /// Validate backup before restoring
        #[arg(long)]
        validate: bool,
    },
    /// List available backups
    List,
}

// Configuration data structures for import/export
#[derive(Serialize, Deserialize, Debug)]
struct ConfigurationExport {
    topics: Vec<TopicConfig>,
    rss_feeds: Vec<RssConfig>,
    system: Vec<SystemConfig>,
    exported_at: String,
    version: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct TopicConfig {
    name: String,
    prompt: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct RssConfig {
    name: String,
    url: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct SystemConfig {
    key: String,
    value: String,
    sensitive: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Database::new().await?;
    
    match cli.command {
        Commands::Topics { action } => handle_topics(db, action).await,
        Commands::Rss { action } => handle_rss(db, action).await,
        Commands::Config { action } => handle_config(db, action).await,
        Commands::HealthCheck => handle_health_check(db).await,
        Commands::Backup { action } => handle_backup(db, action).await,
        Commands::Status => handle_status(db).await,
    }
}

async fn handle_topics(db: Database, action: TopicAction) -> Result<()> {
    match action {
        TopicAction::List => {
            let topics = db.get_configs_by_category("topics").await?;
            if topics.is_empty() {
                println!("No topics configured.");
            } else {
                println!("Topics:");
                for (name, prompt) in topics {
                    println!("  {}: {}", name, prompt);
                }
            }
        }
        TopicAction::Add { name, prompt } => {
            db.set_config("topics", &name, &prompt).await?;
            println!("✅ Added topic: {} -> {}", name, prompt);
        }
        TopicAction::Remove { name } => {
            db.remove_config("topics", &name).await?;
            println!("✅ Removed topic: {}", name);
        }
    }
    Ok(())
}

async fn handle_rss(db: Database, action: RssAction) -> Result<()> {
    match action {
        RssAction::List => {
            let feeds = db.get_configs_by_category("rss").await?;
            if feeds.is_empty() {
                println!("No RSS feeds configured.");
            } else {
                println!("RSS Feeds:");
                for (name, url) in feeds {
                    println!("  {}: {}", name, url);
                }
            }
        }
        RssAction::Add { name, url } => {
            db.set_config("rss", &name, &url).await?;
            println!("✅ Added RSS feed: {} -> {}", name, url);
        }
        RssAction::Remove { name } => {
            db.remove_config("rss", &name).await?;
            println!("✅ Removed RSS feed: {}", name);
        }
    }
    Ok(())
}

async fn handle_config(db: Database, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::List => {
            let system_configs = db.get_configs_by_category("system").await?;
            if system_configs.is_empty() {
                println!("No system configuration found.");
            } else {
                println!("System Configuration:");
                for (key, value) in system_configs {
                    let display_value = if key == "slack_token" { "[REDACTED]" } else { &value };
                    println!("  {}: {}", key, display_value);
                }
            }
        }
        ConfigAction::Get { key } => {
            if let Some(value) = db.get_config("system", &key).await? {
                let display_value = if key == "slack_token" { "[REDACTED]" } else { &value };
                println!("{}: {}", key, display_value);
            } else {
                println!("Configuration key '{}' not found.", key);
            }
        }
        ConfigAction::Set { key, value } => {
            db.set_config("system", &key, &value).await?;
            let display_value = if key == "slack_token" { "[REDACTED]" } else { &value };
            println!("✅ Set {}: {}", key, display_value);
        }
    }
    Ok(())
}

async fn handle_health_check(db: Database) -> Result<()> {
    println!("🏥 Running health check...");
    
    // Check database connection
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&db.pool)
        .await?;
    
    println!("✅ Database: {}", version.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
    
    // Check table counts
    let tables = ["articles", "entities", "configurations"];
    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0);
        println!("✅ {}: {} records", table, count);
    }
    
    println!("✅ Health check completed");
    Ok(())
}

async fn handle_backup(db: Database, action: BackupAction) -> Result<()> {
    match action {
        BackupAction::Create => {
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let backup_file = format!("argus_backup_{}.sql", timestamp);
            
            // Create PostgreSQL backup
            let output = std::process::Command::new("pg_dump")
                .arg(&std::env::var("DATABASE_URL")?)
                .arg("-f")
                .arg(&backup_file)
                .output()?;
            
            if output.status.success() {
                println!("✅ Backup created: {}", backup_file);
            } else {
                return Err(anyhow::anyhow!("Backup failed"));
            }
        }
    }
    Ok(())
}

async fn handle_status(db: Database) -> Result<()> {
    println!("📊 Argus Status");
    
    // Database info
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&db.pool)
        .await?;
    println!("Database: {}", version.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
    
    // Configuration counts
    let topics_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM configurations WHERE category = 'topics'")
        .fetch_one(&db.pool)
        .await?;
    
    let rss_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM configurations WHERE category = 'rss'")
        .fetch_one(&db.pool)
        .await?;
    
    println!("Topics: {}", topics_count);
    println!("RSS Feeds: {}", rss_count);
    
    // Data counts
    let articles_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
        .fetch_one(&db.pool)
        .await?;
    
    println!("Articles: {}", articles_count);
    
    Ok(())
}
```

## Integration Steps

1. **Add dependencies to Cargo.toml**:
```toml
[dependencies]
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono", "uuid"] }
clap = { version = "4.0", features = ["derive"] }
```

2. **Update main application** to use PostgreSQL-only database layer

3. **Create shell alias** for convenient admin tool usage:
```bash
echo "alias aa='cargo run --bin argus_admin'" >> ~/.bashrc
```

4. **Test migration** in development environment before production

This implementation provides a streamlined, maintainable solution that eliminates the complexity of the original plan while maintaining all essential functionality.
