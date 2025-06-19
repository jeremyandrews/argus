use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use std::fs;
use std::path::PathBuf;
use tokio::time::Duration;

#[derive(Parser)]
#[command(name = "argus_admin")]
#[command(
    about = "Argus administration tool with bulk operations, validation, and dry-run support"
)]
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
        /// Preview restore without applying
        #[arg(long)]
        dry_run: bool,
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

    if cli.dry_run {
        println!("🔍 DRY-RUN MODE: No changes will be applied");
    }

    let pool = setup_database_connection().await?;

    match cli.command {
        Commands::Topics { action } => handle_topics(pool, action, cli.dry_run).await,
        Commands::Rss { action } => handle_rss(pool, action, cli.dry_run).await,
        Commands::Config { action } => handle_config(pool, action, cli.dry_run).await,
        Commands::HealthCheck => handle_health_check(pool).await,
        Commands::Backup { action } => handle_backup(pool, action, cli.dry_run).await,
        Commands::Status => handle_status(pool).await,
        Commands::ValidateAll => handle_validate_all(pool).await,
    }
}

async fn setup_database_connection() -> Result<Pool<Postgres>> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable not set"))?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&database_url)
        .await?;

    Ok(pool)
}

async fn handle_topics(pool: Pool<Postgres>, action: TopicAction, dry_run: bool) -> Result<()> {
    match action {
        TopicAction::List => {
            let topics = get_configs_by_category(&pool, "topics").await?;
            if topics.is_empty() {
                println!("No topics configured.");
            } else {
                println!("Topics:");
                for (name, prompt) in topics {
                    println!("  {}: {}", name, prompt);
                }
            }
        }
        TopicAction::Add {
            name,
            prompt,
            validate,
        } => {
            if validate {
                validate_topic(&name, &prompt)?;
            }

            if dry_run {
                println!("Would add topic: {} -> {}", name, prompt);
            } else {
                set_config(&pool, "topics", &name, &prompt).await?;
                println!("✅ Added topic: {} -> {}", name, prompt);
            }
        }
        TopicAction::Remove { name } => {
            if dry_run {
                println!("Would remove topic: {}", name);
            } else {
                remove_config(&pool, "topics", &name).await?;
                println!("✅ Removed topic: {}", name);
            }
        }
        TopicAction::Export { file } => {
            let topics = export_topics(&pool).await?;
            let json = serde_json::to_string_pretty(&topics)?;
            fs::write(&file, json)?;
            println!("✅ Exported {} topics to {}", topics.len(), file.display());
        }
        TopicAction::Import { file, validate } => {
            let content = fs::read_to_string(&file)?;
            let topics: Vec<TopicConfig> = serde_json::from_str(&content)?;

            if validate {
                for topic in &topics {
                    validate_topic(&topic.name, &topic.prompt)?;
                }
            }

            if dry_run {
                println!(
                    "Would import {} topics from {}",
                    topics.len(),
                    file.display()
                );
                for topic in &topics {
                    println!("  Would add: {} -> {}", topic.name, topic.prompt);
                }
            } else {
                for topic in topics {
                    if topic.enabled {
                        set_config(&pool, "topics", &topic.name, &topic.prompt).await?;
                        println!("✅ Imported topic: {}", topic.name);
                    }
                }
            }
        }
        TopicAction::Validate { name, prompt } => match validate_topic(&name, &prompt) {
            Ok(_) => println!("✅ Topic '{}' is valid", name),
            Err(e) => println!("❌ Topic '{}' is invalid: {}", name, e),
        },
    }
    Ok(())
}

async fn handle_rss(pool: Pool<Postgres>, action: RssAction, dry_run: bool) -> Result<()> {
    match action {
        RssAction::List => {
            let feeds = get_configs_by_category(&pool, "rss").await?;
            if feeds.is_empty() {
                println!("No RSS feeds configured.");
            } else {
                println!("RSS Feeds:");
                for (name, url) in feeds {
                    println!("  {}: {}", name, url);
                }
            }
        }
        RssAction::Add {
            name,
            url,
            validate,
        } => {
            if validate {
                validate_rss_url(&url).await?;
            }

            if dry_run {
                println!("Would add RSS feed: {} -> {}", name, url);
            } else {
                set_config(&pool, "rss", &name, &url).await?;
                println!("✅ Added RSS feed: {} -> {}", name, url);
            }
        }
        RssAction::Remove { name } => {
            if dry_run {
                println!("Would remove RSS feed: {}", name);
            } else {
                remove_config(&pool, "rss", &name).await?;
                println!("✅ Removed RSS feed: {}", name);
            }
        }
        RssAction::Export { file } => {
            let feeds = export_rss_feeds(&pool).await?;
            let json = serde_json::to_string_pretty(&feeds)?;
            fs::write(&file, json)?;
            println!(
                "✅ Exported {} RSS feeds to {}",
                feeds.len(),
                file.display()
            );
        }
        RssAction::Import { file, validate } => {
            let content = fs::read_to_string(&file)?;
            let feeds: Vec<RssConfig> = serde_json::from_str(&content)?;

            if validate {
                for feed in &feeds {
                    validate_rss_url(&feed.url).await?;
                }
            }

            if dry_run {
                println!(
                    "Would import {} RSS feeds from {}",
                    feeds.len(),
                    file.display()
                );
                for feed in &feeds {
                    println!("  Would add: {} -> {}", feed.name, feed.url);
                }
            } else {
                for feed in feeds {
                    if feed.enabled {
                        set_config(&pool, "rss", &feed.name, &feed.url).await?;
                        println!("✅ Imported RSS feed: {}", feed.name);
                    }
                }
            }
        }
        RssAction::Validate { url } => match validate_rss_url(&url).await {
            Ok(_) => println!("✅ RSS feed URL is valid: {}", url),
            Err(e) => println!("❌ RSS feed URL is invalid: {}", e),
        },
    }
    Ok(())
}

async fn handle_config(pool: Pool<Postgres>, action: ConfigAction, dry_run: bool) -> Result<()> {
    match action {
        ConfigAction::List => {
            let system_configs = get_configs_by_category(&pool, "system").await?;
            if system_configs.is_empty() {
                println!("No system configuration found.");
            } else {
                println!("System Configuration:");
                for (key, value) in system_configs {
                    let display_value = if key.contains("token") {
                        "[REDACTED]"
                    } else {
                        &value
                    };
                    println!("  {}: {}", key, display_value);
                }
            }
        }
        ConfigAction::Get { key } => {
            if let Some(value) = get_config(&pool, "system", &key).await? {
                let display_value = if key.contains("token") {
                    "[REDACTED]"
                } else {
                    &value
                };
                println!("{}: {}", key, display_value);
            } else {
                println!("Configuration key '{}' not found.", key);
            }
        }
        ConfigAction::Set {
            key,
            value,
            validate,
        } => {
            if validate {
                validate_system_config(&key, &value)?;
            }

            if dry_run {
                let display_value = if key.contains("token") {
                    "[REDACTED]"
                } else {
                    &value
                };
                println!("Would set {}: {}", key, display_value);
            } else {
                set_config(&pool, "system", &key, &value).await?;
                let display_value = if key.contains("token") {
                    "[REDACTED]"
                } else {
                    &value
                };
                println!("✅ Set {}: {}", key, display_value);
            }
        }
        ConfigAction::Export {
            file,
            include_sensitive,
        } => {
            let config = export_all_config(&pool, include_sensitive).await?;
            let json = serde_json::to_string_pretty(&config)?;
            fs::write(&file, json)?;
            println!("✅ Exported configuration to {}", file.display());
        }
        ConfigAction::Import { file, validate } => {
            let content = fs::read_to_string(&file)?;
            let config: ConfigurationExport = serde_json::from_str(&content)?;

            if validate {
                for topic in &config.topics {
                    validate_topic(&topic.name, &topic.prompt)?;
                }
                for feed in &config.rss_feeds {
                    validate_rss_url(&feed.url).await?;
                }
                for sys_config in &config.system {
                    validate_system_config(&sys_config.key, &sys_config.value)?;
                }
            }

            if dry_run {
                println!("Would import configuration from {}:", file.display());
                println!("  Topics: {}", config.topics.len());
                println!("  RSS Feeds: {}", config.rss_feeds.len());
                println!("  System Settings: {}", config.system.len());
            } else {
                import_all_config(&pool, config).await?;
                println!("✅ Imported configuration from {}", file.display());
            }
        }
    }
    Ok(())
}

async fn handle_health_check(pool: Pool<Postgres>) -> Result<()> {
    println!("🏥 Running health check...");

    // Check database connection
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await?;

    println!(
        "✅ Database: {}",
        version
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Check table counts
    let tables = ["articles", "entities", "configurations"];
    for table in tables {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
        println!("✅ {}: {} records", table, count);
    }

    println!("✅ Health check completed");
    Ok(())
}

async fn handle_backup(pool: Pool<Postgres>, action: BackupAction, dry_run: bool) -> Result<()> {
    match action {
        BackupAction::Create {
            include_config,
            file,
        } => {
            // Validate database connection first
            let _version: String = sqlx::query_scalar("SELECT version()")
                .fetch_one(&pool)
                .await?;

            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let backup_file =
                file.unwrap_or_else(|| PathBuf::from(format!("argus_backup_{}.sql", timestamp)));

            if dry_run {
                println!("Would create backup: {}", backup_file.display());
                if include_config {
                    println!("  Would include configuration data");
                }
            } else {
                println!("🔍 Validating database connection...");

                // Create PostgreSQL backup
                let output = std::process::Command::new("pg_dump")
                    .arg(&env::var("DATABASE_URL")?)
                    .arg("-f")
                    .arg(&backup_file)
                    .output()?;

                if output.status.success() {
                    println!("✅ Backup created: {}", backup_file.display());

                    // If include_config is requested, also export configuration
                    if include_config {
                        let config_file = format!("argus_config_{}.json", timestamp);
                        let config = export_all_config(&pool, true).await?;
                        let json = serde_json::to_string_pretty(&config)?;
                        fs::write(&config_file, json)?;
                        println!("✅ Configuration exported to: {}", config_file);
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Backup failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            }
        }
        BackupAction::Restore {
            file,
            validate,
            dry_run: restore_dry_run,
        } => {
            if validate {
                if !file.exists() {
                    return Err(anyhow::anyhow!(
                        "Backup file does not exist: {}",
                        file.display()
                    ));
                }
            }

            let actual_dry_run = dry_run || restore_dry_run;

            if actual_dry_run {
                println!("Would restore from backup: {}", file.display());
            } else {
                println!("⚠️  This will overwrite the current database. Are you sure? (y/N)");
                // In a real implementation, you'd prompt for confirmation
                println!("Skipping restore for safety. Use --force flag in production version.");
            }
        }
        BackupAction::List => {
            let backups = list_backup_files()?;
            if backups.is_empty() {
                println!("No backup files found.");
            } else {
                println!("Available backups:");
                for backup in backups {
                    println!("  {}", backup.display());
                }
            }
        }
    }
    Ok(())
}

async fn handle_status(pool: Pool<Postgres>) -> Result<()> {
    println!("📊 Argus Status");

    // Database info
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await?;
    println!(
        "Database: {}",
        version
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Configuration counts
    let topics_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM configurations WHERE category = 'topics' AND enabled = true",
    )
    .fetch_one(&pool)
    .await?;

    let rss_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM configurations WHERE category = 'rss' AND enabled = true",
    )
    .fetch_one(&pool)
    .await?;

    println!("Topics: {}", topics_count);
    println!("RSS Feeds: {}", rss_count);

    // Data counts
    let articles_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
        .fetch_one(&pool)
        .await?;

    println!("Articles: {}", articles_count);

    Ok(())
}

async fn handle_validate_all(pool: Pool<Postgres>) -> Result<()> {
    println!("🔍 Validating all configuration...");

    let mut errors = 0;

    // Validate topics
    let topics = get_configs_by_category(&pool, "topics").await?;
    for (name, prompt) in topics {
        if let Err(e) = validate_topic(&name, &prompt) {
            println!("❌ Topic '{}': {}", name, e);
            errors += 1;
        }
    }

    // Validate RSS feeds
    let feeds = get_configs_by_category(&pool, "rss").await?;
    for (name, url) in feeds {
        if let Err(e) = validate_rss_url(&url).await {
            println!("❌ RSS feed '{}': {}", name, e);
            errors += 1;
        }
    }

    // Validate system config
    let system_configs = get_configs_by_category(&pool, "system").await?;
    for (key, value) in system_configs {
        if let Err(e) = validate_system_config(&key, &value) {
            println!("❌ System config '{}': {}", key, e);
            errors += 1;
        }
    }

    if errors == 0 {
        println!("✅ All configuration is valid");
    } else {
        println!("❌ Found {} validation errors", errors);
    }

    Ok(())
}

// Database helper functions
async fn get_config(pool: &Pool<Postgres>, category: &str, name: &str) -> Result<Option<String>> {
    let result = sqlx::query_scalar(
        "SELECT value FROM configurations 
         WHERE category = $1 AND name = $2 AND enabled = true",
    )
    .bind(category)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(result)
}

async fn get_configs_by_category(
    pool: &Pool<Postgres>,
    category: &str,
) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT name, value FROM configurations 
         WHERE category = $1 AND enabled = true ORDER BY name",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;

    Ok(rows)
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

async fn remove_config(pool: &Pool<Postgres>, category: &str, name: &str) -> Result<()> {
    sqlx::query(
        "UPDATE configurations SET enabled = false, updated_at = NOW()
         WHERE category = $1 AND name = $2",
    )
    .bind(category)
    .bind(name)
    .execute(pool)
    .await?;

    Ok(())
}

// Export/Import functions
async fn export_topics(pool: &Pool<Postgres>) -> Result<Vec<TopicConfig>> {
    let topics = get_configs_by_category(pool, "topics").await?;
    Ok(topics
        .into_iter()
        .map(|(name, prompt)| TopicConfig {
            name,
            prompt,
            enabled: true,
        })
        .collect())
}

async fn export_rss_feeds(pool: &Pool<Postgres>) -> Result<Vec<RssConfig>> {
    let feeds = get_configs_by_category(pool, "rss").await?;
    Ok(feeds
        .into_iter()
        .map(|(name, url)| RssConfig {
            name,
            url,
            enabled: true,
        })
        .collect())
}

async fn export_all_config(
    pool: &Pool<Postgres>,
    include_sensitive: bool,
) -> Result<ConfigurationExport> {
    let topics = export_topics(pool).await?;
    let rss_feeds = export_rss_feeds(pool).await?;

    let system_configs = get_configs_by_category(pool, "system").await?;
    let system = system_configs
        .into_iter()
        .map(|(key, value)| {
            let sensitive = key.contains("token") || key.contains("password");
            SystemConfig {
                key,
                value: if sensitive && !include_sensitive {
                    "[REDACTED]".to_string()
                } else {
                    value
                },
                sensitive,
            }
        })
        .collect();

    Ok(ConfigurationExport {
        topics,
        rss_feeds,
        system,
        exported_at: chrono::Utc::now().to_rfc3339(),
        version: "1.0".to_string(),
    })
}

async fn import_all_config(pool: &Pool<Postgres>, config: ConfigurationExport) -> Result<()> {
    for topic in config.topics {
        if topic.enabled {
            set_config(pool, "topics", &topic.name, &topic.prompt).await?;
        }
    }

    for feed in config.rss_feeds {
        if feed.enabled {
            set_config(pool, "rss", &feed.name, &feed.url).await?;
        }
    }

    for sys_config in config.system {
        if !sys_config.value.contains("[REDACTED]") {
            set_config(pool, "system", &sys_config.key, &sys_config.value).await?;
        }
    }

    Ok(())
}

// Validation functions
fn validate_topic(name: &str, prompt: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(anyhow::anyhow!("Topic name cannot be empty"));
    }
    if prompt.trim().is_empty() {
        return Err(anyhow::anyhow!("Topic prompt cannot be empty"));
    }
    if name.len() > 100 {
        return Err(anyhow::anyhow!("Topic name too long (max 100 characters)"));
    }
    if prompt.len() > 1000 {
        return Err(anyhow::anyhow!(
            "Topic prompt too long (max 1000 characters)"
        ));
    }
    Ok(())
}

async fn validate_rss_url(url: &str) -> Result<()> {
    if url.trim().is_empty() {
        return Err(anyhow::anyhow!("RSS URL cannot be empty"));
    }

    // Basic URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow::anyhow!(
            "RSS URL must start with http:// or https://"
        ));
    }

    // Try to fetch the URL to validate it's accessible
    let client = reqwest::Client::new();
    let response = client.head(url).send().await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Err(anyhow::anyhow!(
                    "RSS URL returned status: {}",
                    resp.status()
                ));
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Cannot access RSS URL: {}", e));
        }
    }

    Ok(())
}

fn validate_system_config(key: &str, value: &str) -> Result<()> {
    if key.trim().is_empty() {
        return Err(anyhow::anyhow!("Configuration key cannot be empty"));
    }
    if value.trim().is_empty() {
        return Err(anyhow::anyhow!("Configuration value cannot be empty"));
    }

    // Specific validation for known keys
    match key {
        "no_think_mode" => {
            if !matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0") {
                return Err(anyhow::anyhow!("no_think_mode must be true/false or 1/0"));
            }
        }
        "rust_log" => {
            // Basic RUST_LOG validation
            let valid_levels = ["error", "warn", "info", "debug", "trace"];
            let parts: Vec<&str> = value.split(',').collect();
            for part in parts {
                let level = part.split('=').last().unwrap_or(part);
                if !valid_levels.contains(&level) {
                    return Err(anyhow::anyhow!("Invalid log level in RUST_LOG: {}", level));
                }
            }
        }
        _ => {} // Other keys pass through without specific validation
    }

    Ok(())
}

fn list_backup_files() -> Result<Vec<PathBuf>> {
    let mut backups = Vec::new();

    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("argus_backup_") && name.ends_with(".sql") {
                        backups.push(path);
                    }
                }
            }
        }
    }

    backups.sort();
    Ok(backups)
}
