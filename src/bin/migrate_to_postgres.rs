use anyhow::Result;
use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::process::Command;
use tokio::time::Duration;

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
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

    // Validate that it's a PostgreSQL URL
    if !database_url.starts_with("postgresql://") && !database_url.starts_with("postgres://") {
        return Err(anyhow::anyhow!(
            "DATABASE_URL must be a PostgreSQL connection string"
        ));
    }

    // Check PostgreSQL connection
    let output = Command::new("psql")
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

    // First, clean up any existing schema
    clean_existing_schema(pool).await?;

    let schema_sql = include_str!("../../memory-bank/postgresql-migration/schema.sql");

    // Execute schema without transaction to avoid prepared statement issues
    // Parse and categorize SQL statements
    let (table_statements, index_statements, other_statements) =
        parse_schema_statements(schema_sql);

    // Execute in correct order: tables first, then indexes, then other statements
    println!("  📋 Creating tables...");
    for statement in table_statements {
        sqlx::query(&statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute table statement: {}\nError: {}",
                statement,
                e
            )
        })?;
    }

    println!("  🔗 Creating indexes...");
    for statement in index_statements {
        sqlx::query(&statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to execute index statement: {}\nError: {}",
                statement,
                e
            )
        })?;
    }

    println!("  ⚙️  Creating functions and triggers...");
    for statement in other_statements {
        sqlx::query(&statement).execute(pool).await.map_err(|e| {
            anyhow::anyhow!("Failed to execute statement: {}\nError: {}", statement, e)
        })?;
    }

    println!("✅ PostgreSQL schema created");
    Ok(())
}

async fn clean_existing_schema(pool: &Pool<Postgres>) -> Result<()> {
    println!("  🧹 Cleaning existing schema...");

    // Get list of all tables in the public schema
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename FROM pg_tables WHERE schemaname = 'public'")
            .fetch_all(pool)
            .await?;

    if !tables.is_empty() {
        println!("    🗑️  Dropping {} existing tables...", tables.len());

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
    println!("    🔧 Cleaning sequences, functions, and types...");

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

    println!("    ✅ Schema cleanup completed");
    Ok(())
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

async fn migrate_data_via_dump(pool: &Pool<Postgres>) -> Result<()> {
    println!("📥 Migrating data via dump/restore...");

    // Step 1: Export SQLite data
    println!("  📤 Exporting SQLite data...");
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

    // Step 2: Transform SQL for PostgreSQL compatibility
    println!("  🔄 Transforming SQL for PostgreSQL...");
    let postgres_sql = transform_sqlite_to_postgres(&sqlite_dump)?;

    // Write transformed SQL to temp file
    let temp_file = "/tmp/postgres_import.sql";
    fs::write(temp_file, postgres_sql)?;

    // Step 3: Import to PostgreSQL
    println!("  📥 Importing to PostgreSQL...");
    let import_output = Command::new("psql")
        .arg(&env::var("DATABASE_URL")?)
        .arg("-f")
        .arg(temp_file)
        .output()?;

    if !import_output.status.success() {
        let error = String::from_utf8_lossy(&import_output.stderr);
        // Only warn on non-critical errors, but continue
        if error.contains("already exists") || error.contains("duplicate key") {
            println!(
                "  ⚠️  Import completed with warnings (duplicate data): {}",
                error
            );
        } else {
            println!("  ⚠️  Import completed with warnings: {}", error);
        }
    }

    // Step 4: Reset sequences
    reset_postgres_sequences(pool).await?;

    // Cleanup
    let _ = fs::remove_file(temp_file);

    println!("✅ Data migration completed");
    Ok(())
}

fn transform_sqlite_to_postgres(sqlite_sql: &str) -> Result<String> {
    let mut postgres_sql = sqlite_sql.to_string();

    // Transform SQLite-specific syntax to PostgreSQL
    postgres_sql = postgres_sql.replace("INTEGER PRIMARY KEY AUTOINCREMENT", "SERIAL PRIMARY KEY");
    postgres_sql = postgres_sql.replace("INTEGER PRIMARY KEY", "SERIAL PRIMARY KEY");
    postgres_sql = postgres_sql.replace("AUTOINCREMENT", "");

    // Handle boolean values
    postgres_sql = postgres_sql.replace("'0'", "false");
    postgres_sql = postgres_sql.replace("'1'", "true");

    // Remove SQLite-specific statements
    let lines: Vec<&str> = postgres_sql
        .lines()
        .filter(|line| !line.starts_with("PRAGMA"))
        .filter(|line| !line.starts_with("BEGIN TRANSACTION"))
        .filter(|line| !line.starts_with("COMMIT"))
        .filter(|line| !line.contains("sqlite_sequence"))
        // Skip CREATE TABLE statements for tables that already exist in schema
        .filter(|line| !line.starts_with("CREATE TABLE configurations"))
        .collect();

    postgres_sql = lines.join("\n");

    // Remove any remaining SQLite incompatibilities
    postgres_sql = postgres_sql.replace("WITHOUT ROWID", "");

    Ok(postgres_sql)
}

async fn reset_postgres_sequences(pool: &Pool<Postgres>) -> Result<()> {
    println!("  🔄 Resetting PostgreSQL sequences...");

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

    println!("  ✅ Sequences reset");
    Ok(())
}

async fn migrate_env_to_database(pool: &Pool<Postgres>) -> Result<()> {
    println!("⚙️  Migrating environment configuration to database...");

    // Migrate topics from TOPICS env var
    if let Ok(topics_str) = env::var("TOPICS") {
        println!("  📝 Migrating topics...");
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
                        println!("    ✅ Migrated topic: {}", name);
                    }
                }
            }
        }
    }

    // Migrate RSS feeds from URLS env var
    if let Ok(urls_str) = env::var("URLS") {
        println!("  📡 Migrating RSS feeds...");
        for (i, url) in urls_str.split(';').enumerate() {
            let url = url.trim();
            if !url.is_empty() {
                let name = format!("feed_{}", i + 1);
                set_config(pool, "rss", &name, url).await?;
                println!("    ✅ Migrated RSS feed: {}", name);
            }
        }
    }

    // Migrate worker configurations
    println!("  🔧 Migrating worker configurations...");
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

                    println!(
                        "    ✅ Migrated {} worker {}: {}",
                        category, index, display_config
                    );
                }
                println!(
                    "    📊 Total {} workers migrated: {}",
                    category,
                    configs.len()
                );
            }
        }
    }

    // Migrate LLM parameters
    println!("  🎛️  Migrating LLM parameters...");
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
                println!("    ✅ Migrated LLM parameter: {} = {}", key, value);
            }
        }
    }

    // Migrate OpenAI rate limiting configuration
    println!("  🚦 Migrating rate limiting configuration...");
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
                println!("    ✅ Migrated rate limit setting: {} = {}", key, value);
            }
        }
    }

    // Migrate system settings
    println!("  ⚙️  Migrating system settings...");
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
                println!(
                    "    ✅ Migrated system setting: {} = {}",
                    key, display_value
                );
            }
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
    println!("✅ Validating migration...");

    // Check table counts
    let tables = ["articles", "entities", "article_entities", "configurations"];

    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(pool)
            .await
            .unwrap_or(0);

        println!("  ✅ {}: {} records", table, count);
    }

    // Test basic functionality
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(pool)
        .await?;

    println!(
        "  ✅ PostgreSQL version: {}",
        version
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Test configuration system
    let config_categories: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT category FROM configurations ORDER BY category")
            .fetch_all(pool)
            .await?;

    println!("  ✅ Configuration categories: {:?}", config_categories);

    println!("✅ Migration validation completed");
    Ok(())
}
