use anyhow::Result;
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use tokio::time::Duration;

#[derive(Parser)]
#[command(name = "test_postgres_migration")]
#[command(about = "Test and validate PostgreSQL migration")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Test database connection only
    #[command(name = "test-connection")]
    TestConnection,
    /// Test data migration (articles, entities, etc.)
    #[command(name = "test-data")]
    TestData,
    /// Test configuration migration
    #[command(name = "test-config")]
    TestConfig,
    /// Test worker configurations can be parsed
    #[command(name = "test-workers")]
    TestWorkers,
    /// Run all tests
    #[command(name = "test-all")]
    TestAll,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let command = cli.command.unwrap_or(Commands::TestAll);

    match command {
        Commands::TestConnection => test_connection().await,
        Commands::TestData => test_data_migration().await,
        Commands::TestConfig => test_config_migration().await,
        Commands::TestWorkers => test_worker_configs().await,
        Commands::TestAll => {
            println!("🧪 Running comprehensive PostgreSQL migration tests...\n");
            test_connection().await?;
            test_data_migration().await?;
            test_config_migration().await?;
            test_worker_configs().await?;
            println!("\n✅ All migration tests passed!");
            Ok(())
        }
    }
}

async fn get_database_pool() -> Result<Pool<Postgres>> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&database_url)
        .await?;

    Ok(pool)
}

async fn test_connection() -> Result<()> {
    println!("🔌 Testing PostgreSQL connection...");

    let pool = get_database_pool().await?;

    // Test basic connection
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await?;

    println!("  ✅ Connection successful");
    println!(
        "  ✅ PostgreSQL version: {}",
        version
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Test schema exists
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables 
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE'",
    )
    .fetch_one(&pool)
    .await?;

    println!("  ✅ Schema loaded: {} tables found", table_count);

    if table_count < 10 {
        return Err(anyhow::anyhow!(
            "Expected at least 10 tables, found {}. Schema may not be fully loaded.",
            table_count
        ));
    }

    Ok(())
}

async fn test_data_migration() -> Result<()> {
    println!("📊 Testing data migration...");

    let pool = get_database_pool().await?;

    // Test core tables
    let core_tables = [
        "articles",
        "entities",
        "article_entities",
        "entity_aliases",
        "article_clusters",
        "article_cluster_mappings",
        "devices",
    ];

    let mut total_records = 0;

    for table in core_tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

        println!("  ✅ {}: {} records", table, count);
        total_records += count;
    }

    println!("  📊 Total data records migrated: {}", total_records);

    // Test specific data integrity
    if total_records > 0 {
        // Test articles have required fields
        let articles_with_urls: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM articles WHERE url IS NOT NULL AND url != ''")
                .fetch_one(&pool)
                .await?;

        let total_articles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
            .fetch_one(&pool)
            .await?;

        println!(
            "  ✅ Articles with URLs: {}/{} ({:.1}%)",
            articles_with_urls,
            total_articles,
            if total_articles > 0 {
                (articles_with_urls as f64 / total_articles as f64) * 100.0
            } else {
                0.0
            }
        );

        // Test JSONB fields work
        let articles_with_analysis: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM articles WHERE analysis IS NOT NULL")
                .fetch_one(&pool)
                .await?;

        println!(
            "  ✅ Articles with analysis data: {}",
            articles_with_analysis
        );

        // Test entity relationships
        let entity_relationships: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM article_entities")
            .fetch_one(&pool)
            .await?;

        println!("  ✅ Entity relationships: {}", entity_relationships);
    }

    Ok(())
}

async fn test_config_migration() -> Result<()> {
    println!("⚙️  Testing configuration migration...");

    let pool = get_database_pool().await?;

    // Test configuration table exists and has data
    let config_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM configurations")
        .fetch_one(&pool)
        .await?;

    println!("  ✅ Total configurations: {}", config_count);

    if config_count == 0 {
        return Err(anyhow::anyhow!(
            "No configurations found. Migration may have failed."
        ));
    }

    // Test configuration categories
    let categories: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT category FROM configurations ORDER BY category")
            .fetch_all(&pool)
            .await?;

    println!("  ✅ Configuration categories: {:?}", categories);

    // Test each category has data
    for category in &categories {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM configurations WHERE category = $1")
                .bind(category)
                .fetch_one(&pool)
                .await?;

        println!("    📂 {}: {} items", category, count);
    }

    // Test specific configurations exist
    let expected_categories = ["topics", "rss", "system"];
    for expected in expected_categories {
        if !categories.contains(&expected.to_string()) {
            println!("    ⚠️  Missing expected category: {}", expected);
        }
    }

    // Test configuration audit trail
    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM configuration_changes")
        .fetch_one(&pool)
        .await?;

    println!("  ✅ Configuration audit trail: {} entries", audit_count);

    Ok(())
}

async fn test_worker_configs() -> Result<()> {
    println!("🔧 Testing worker configurations...");

    let pool = get_database_pool().await?;

    // Test decision worker configs
    let decision_configs: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, value FROM configurations WHERE category = 'decision' ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

    println!(
        "  🎯 Decision workers: {} configurations",
        decision_configs.len()
    );

    for (name, config) in &decision_configs {
        println!("    📝 {}: {}", name, mask_sensitive_config(config));

        // Validate config format
        if let Err(e) = validate_worker_config(config) {
            println!("      ⚠️  Invalid config format: {}", e);
        } else {
            println!("      ✅ Valid config format");
        }
    }

    // Test analysis worker configs
    let analysis_configs: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, value FROM configurations WHERE category = 'analysis' ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

    println!(
        "  🔍 Analysis workers: {} configurations",
        analysis_configs.len()
    );

    for (name, config) in &analysis_configs {
        println!("    📝 {}: {}", name, mask_sensitive_config(config));

        // Validate config format
        if let Err(e) = validate_worker_config(config) {
            println!("      ⚠️  Invalid config format: {}", e);
        } else {
            println!("      ✅ Valid config format");
        }
    }

    // Test LLM parameters
    let llm_params: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, value FROM configurations WHERE category = 'llm_params' ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

    println!("  🎛️  LLM parameters: {} configurations", llm_params.len());

    for (name, value) in &llm_params {
        println!("    📝 {}: {}", name, value);

        // Validate parameter values
        if let Err(e) = validate_llm_param(name, value) {
            println!("      ⚠️  Invalid parameter value: {}", e);
        } else {
            println!("      ✅ Valid parameter value");
        }
    }

    // Test rate limiting configs
    let rate_limit_configs: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, value FROM configurations WHERE category = 'rate_limit' ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

    println!(
        "  🚦 Rate limiting: {} configurations",
        rate_limit_configs.len()
    );

    for (name, value) in &rate_limit_configs {
        println!("    📝 {}: {}", name, value);
    }

    Ok(())
}

fn mask_sensitive_config(config: &str) -> String {
    // Mask OpenAI API keys
    if config.contains("sk-") {
        let parts: Vec<&str> = config.split('|').collect();
        if parts.len() >= 2 {
            format!("[API_KEY_REDACTED]|{}", parts[1])
        } else {
            "[REDACTED]".to_string()
        }
    } else {
        config.to_string()
    }
}

fn validate_worker_config(config: &str) -> Result<()> {
    if config.contains("sk-") {
        // OpenAI config: api_key|model
        let parts: Vec<&str> = config.split('|').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "OpenAI config must have format: api_key|model"
            ));
        }
        if !parts[0].starts_with("sk-") {
            return Err(anyhow::anyhow!("OpenAI API key must start with 'sk-'"));
        }
        if parts[1].is_empty() {
            return Err(anyhow::anyhow!("Model name cannot be empty"));
        }
    } else {
        // Ollama config: host|port|model[/no_think]
        let main_parts: Vec<&str> = config.split("||").collect();

        for part in main_parts {
            let config_parts: Vec<&str> = part.split('|').collect();
            if config_parts.len() != 3 {
                return Err(anyhow::anyhow!(
                    "Ollama config must have format: host|port|model"
                ));
            }

            // Validate port is numeric
            if config_parts[1].parse::<u16>().is_err() {
                return Err(anyhow::anyhow!("Port must be a valid number"));
            }

            if config_parts[2].is_empty() {
                return Err(anyhow::anyhow!("Model name cannot be empty"));
            }
        }
    }

    Ok(())
}

fn validate_llm_param(name: &str, value: &str) -> Result<()> {
    match name {
        "llm_temperature" => {
            let temp: f32 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Temperature must be a number"))?;
            if temp < 0.0 || temp > 2.0 {
                return Err(anyhow::anyhow!("Temperature must be between 0.0 and 2.0"));
            }
        }
        "llm_top_p" => {
            let top_p: f32 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Top-P must be a number"))?;
            if top_p < 0.0 || top_p > 1.0 {
                return Err(anyhow::anyhow!("Top-P must be between 0.0 and 1.0"));
            }
        }
        "llm_top_k" => {
            let top_k: i32 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Top-K must be an integer"))?;
            if top_k < 0 {
                return Err(anyhow::anyhow!("Top-K must be non-negative"));
            }
        }
        "llm_min_p" => {
            let min_p: f32 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("Min-P must be a number"))?;
            if min_p < 0.0 || min_p > 1.0 {
                return Err(anyhow::anyhow!("Min-P must be between 0.0 and 1.0"));
            }
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown LLM parameter: {}", name));
        }
    }

    Ok(())
}
