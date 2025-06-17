# 06 - Configuration Migration

**Effort**: S (1 day)

## Simple Environment to Database Migration

### Migration Strategy
Move all environment variables to database storage with simple categorization:
- `topics` - Topic definitions and prompts
- `rss_feeds` - RSS feed URLs
- `system` - System settings (Slack, logging, etc.)

### Environment Variables to Migrate
```bash
# Topics (parse from TOPICS env var)
TOPICS="Tuscany:Tuscany, the famous region in Italy;Space:Space and Space Exploration;..."

# RSS Feeds (parse from URLS env var)  
URLS="http://rss.slashdot.org/Slashdot/slashdot;https://9to5mac.com/rss;..."

# System Settings
SLACK_TOKEN="xoxb-XXXXXXXXXXXX"
SLACK_CHANNEL="CXXXXXXXXX"
PLACES_JSON_PATH="places.json"
RUST_LOG="info"
```

## Migration Implementation

### Enhanced Migration Binary

**Update src/bin/migrate_env_to_db.rs:**
```rust
use anyhow::Result;
use argus::db::Database;
use std::env;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    info!("Starting environment configuration migration to database...");
    
    let db = Database::new().await?;
    
    // Migrate topics
    migrate_topics(&db).await?;
    
    // Migrate RSS feeds
    migrate_rss_feeds(&db).await?;
    
    // Migrate system settings
    migrate_system_settings(&db).await?;
    
    info!("✅ Configuration migration completed successfully!");
    Ok(())
}

async fn migrate_topics(db: &Database) -> Result<()> {
    info!("Migrating topics from TOPICS environment variable...");
    
    let topics_str = env::var("TOPICS").unwrap_or_default();
    let mut count = 0;
    
    for line in topics_str.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        
        for topic_pair in line.split(';') {
            let topic_pair = topic_pair.trim();
            if topic_pair.is_empty() { continue; }
            
            let parts: Vec<&str> = topic_pair.split(':').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim();
                let prompt = parts[1..].join(":").trim().to_string(); // Handle colons in prompts
                
                if !name.is_empty() && !prompt.is_empty() {
                    db.set_config("topics", name, &prompt).await?;
                    count += 1;
                    info!("Migrated topic: {} -> {}", name, prompt);
                }
            }
        }
    }
    
    info!("Successfully migrated {} topics", count);
    Ok(())
}

async fn migrate_rss_feeds(db: &Database) -> Result<()> {
    info!("Migrating RSS feeds from URLS environment variable...");
    
    let urls_str = env::var("URLS").unwrap_or_default();
    let mut count = 0;
    
    for line in urls_str.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        
        for url in line.split(';') {
            let url = url.trim();
            if !url.is_empty() {
                // Extract domain name for the configuration name
                let name = if let Ok(parsed_url) = url::Url::parse(url) {
                    parsed_url.host_str()
                        .unwrap_or("unknown")
                        .replace("www.", "")
                        .replace('.', "_")
                } else {
                    format!("feed_{}", count + 1)
                };
                
                db.set_config("rss_feeds", &name, url).await?;
                count += 1;
                info!("Migrated RSS feed: {} -> {}", name, url);
            }
        }
    }
    
    info!("Successfully migrated {} RSS feeds", count);
    Ok(())
}

async fn migrate_system_settings(db: &Database) -> Result<()> {
    info!("Migrating system settings...");
    let mut count = 0;
    
    let system_vars = [
        ("slack_token", "SLACK_TOKEN"),
        ("slack_channel", "SLACK_CHANNEL"), 
        ("places_json_path", "PLACES_JSON_PATH"),
        ("rust_log", "RUST_LOG"),
    ];
    
    for (key, env_var) in system_vars {
        if let Ok(value) = env::var(env_var) {
            db.set_config("system", key, &value).await?;
            count += 1;
            info!("Migrated system setting: {} -> {}", key, 
                  if key == "slack_token" { "[REDACTED]" } else { &value });
        } else {
            warn!("Environment variable {} not found, skipping", env_var);
        }
    }
    
    info!("Successfully migrated {} system settings", count);
    Ok(())
}
```

## Configuration Validation

### Create src/bin/validate_config.rs
```rust
use anyhow::Result;
use argus::{config::ConfigManager, db::Database};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Validating configuration migration...");
    
    let db = Database::new().await?;
    let config_manager = ConfigManager::new().await?;
    
    // Validate topics
    let topics = config_manager.get_topics().await;
    println!("✅ Found {} topics in database", topics.len());
    for (name, _prompt) in &topics {
        println!("  - {}", name);
    }
    
    // Validate RSS feeds
    let feeds = config_manager.get_rss_feeds().await;
    println!("✅ Found {} RSS feeds in database", feeds.len());
    for feed in &feeds {
        println!("  - {}", feed);
    }
    
    // Validate system settings
    let system_settings = ["slack_token", "slack_channel", "places_json_path", "rust_log"];
    let mut found_settings = 0;
    
    for setting in system_settings {
        if let Some(value) = config_manager.get_system_setting(setting).await {
            found_settings += 1;
            let display_value = if setting == "slack_token" { "[REDACTED]" } else { &value };
            println!("✅ System setting {}: {}", setting, display_value);
        } else {
            println!("⚠️  System setting {} not found", setting);
        }
    }
    
    println!("✅ Found {} system settings in database", found_settings);
    
    // Test configuration retrieval
    if topics.is_empty() {
        println!("❌ No topics found - migration may have failed");
        return Ok(());
    }
    
    if feeds.is_empty() {
        println!("❌ No RSS feeds found - migration may have failed");
        return Ok(());
    }
    
    println!("🎉 Configuration validation completed successfully!");
    Ok(())
}
```

## Runtime Configuration Management

### Topic Management Commands

**Create src/bin/manage_topics.rs:**
```rust
use anyhow::Result;
use argus::config::ConfigManager;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: manage_topics <command> [args...]");
        println!("Commands:");
        println!("  list                    - List all topics");
        println!("  add <name> <prompt>     - Add new topic");
        println!("  remove <name>           - Remove topic");
        return Ok(());
    }
    
    let config_manager = ConfigManager::new().await?;
    
    match args[1].as_str() {
        "list" => {
            let topics = config_manager.get_topics().await;
            println!("Current topics ({}):", topics.len());
            for (name, prompt) in topics {
                println!("  {}: {}", name, prompt);
            }
        }
        
        "add" => {
            if args.len() < 4 {
                println!("Usage: manage_topics add <name> <prompt>");
                return Ok(());
            }
            
            let name = &args[2];
            let prompt = args[3..].join(" ");
            
            config_manager.add_topic(name, &prompt).await?;
            println!("✅ Added topic: {} -> {}", name, prompt);
        }
        
        "remove" => {
            if args.len() < 3 {
                println!("Usage: manage_topics remove <name>");
                return Ok(());
            }
            
            let name = &args[2];
            config_manager.db.set_config("topics", name, "").await?;
            println!("✅ Removed topic: {}", name);
        }
        
        _ => {
            println!("Unknown command: {}", args[1]);
        }
    }
    
    Ok(())
}
```

### RSS Feed Management Commands

**Create src/bin/manage_feeds.rs:**
```rust
use anyhow::Result;
use argus::config::ConfigManager;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: manage_feeds <command> [args...]");
        println!("Commands:");
        println!("  list                 - List all feeds");
        println!("  add <name> <url>     - Add new feed");
        println!("  remove <name>        - Remove feed");
        return Ok(());
    }
    
    let config_manager = ConfigManager::new().await?;
    
    match args[1].as_str() {
        "list" => {
            let feeds = config_manager.get_rss_feeds().await;
            println!("Current RSS feeds ({}):", feeds.len());
            for (i, feed) in feeds.iter().enumerate() {
                println!("  {}: {}", i + 1, feed);
            }
        }
        
        "add" => {
            if args.len() < 4 {
                println!("Usage: manage_feeds add <name> <url>");
                return Ok(());
            }
            
            let name = &args[2];
            let url = &args[3];
            
            config_manager.add_rss_feed(name, url).await?;
            println!("✅ Added RSS feed: {} -> {}", name, url);
        }
        
        "remove" => {
            if args.len() < 3 {
                println!("Usage: manage_feeds remove <name>");
                return Ok(());
            }
            
            let name = &args[2];
            config_manager.db.set_config("rss_feeds", name, "").await?;
            println!("✅ Removed RSS feed: {}", name);
        }
        
        _ => {
            println!("Unknown command: {}", args[1]);
        }
    }
    
    Ok(())
}
```

## Migration Script

### Create migrate_config.sh
```bash
#!/bin/bash
set -e

echo "🚀 Starting configuration migration..."

# Check environment variables exist
if [ -z "$TOPICS" ] || [ -z "$URLS" ]; then
    echo "⚠️  Warning: TOPICS or URLS environment variables not set"
    echo "   Make sure to source your .env file or set variables manually"
fi

# Run migration
echo "📥 Migrating environment variables to database..."
cargo run --bin migrate_env_to_db

# Validate migration
echo "✅ Validating configuration migration..."
cargo run --bin validate_config

echo "🎉 Configuration migration completed!"
echo ""
echo "Configuration management commands:"
echo "  cargo run --bin manage_topics list"
echo "  cargo run --bin manage_feeds list"
echo "  cargo run --bin manage_topics add \"New Topic\" \"Topic description\""
echo "  cargo run --bin manage_feeds add \"feed_name\" \"https://example.com/rss\""
```

## Environment Cleanup

### Post-Migration Environment
After successful migration, update `.env` file:

```bash
# Database Configuration (keep these)
DATABASE_TYPE=postgres
DATABASE_URL=postgresql://argus_user:password@localhost/argus_prod

# REMOVE THESE AFTER MIGRATION (now in database):
# TOPICS="..."
# URLS="..."
# SLACK_TOKEN="..."
# SLACK_CHANNEL="..."
# PLACES_JSON_PATH="..."
# RUST_LOG="..."
```

## Execution Steps
1. **Build migration tools**: `cargo build --bin migrate_env_to_db --bin validate_config --bin manage_topics --bin manage_feeds`
2. **Run migration**: `./migrate_config.sh`
3. **Test runtime management**: 
   - `cargo run --bin manage_topics list`
   - `cargo run --bin manage_feeds list`
4. **Update environment file**: Remove migrated variables
5. **Test application startup**: Ensure app works with database config only
