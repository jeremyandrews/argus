# 07 - Testing & Validation

**Effort**: S (1 day)

## Testing Strategy

### Test Categories
1. **Data Integrity**: Ensure all data migrated correctly
2. **Configuration**: Verify environment-to-database migration works
3. **Performance**: Basic performance validation
4. **Functionality**: All existing features work with PostgreSQL

## Data Integrity Testing

### Enhanced Validation Binary

**Update src/bin/validate_migration.rs:**
```rust
use sqlx::{sqlite::SqlitePoolOptions, postgres::PgPoolOptions};
use std::env;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Starting comprehensive migration validation...");
    
    let sqlite_pool = SqlitePoolOptions::new()
        .connect("sqlite:argus.db")
        .await?;
    
    let postgres_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    let postgres_pool = PgPoolOptions::new()
        .connect(&postgres_url)
        .await?;
    
    // Validate table counts
    validate_table_counts(&sqlite_pool, &postgres_pool).await?;
    
    // Validate data integrity
    validate_data_integrity(&postgres_pool).await?;
    
    // Validate specific data samples
    validate_data_samples(&sqlite_pool, &postgres_pool).await?;
    
    // Validate indexes and constraints
    validate_schema(&postgres_pool).await?;
    
    println!("✅ Migration validation completed successfully!");
    Ok(())
}

async fn validate_table_counts(sqlite: &sqlx::SqlitePool, postgres: &sqlx::PgPool) -> Result<()> {
    println!("\n📊 Validating table counts...");
    
    let tables = [
        "articles", "entities", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings", "cluster_merge_history"
    ];
    
    let mut all_match = true;
    
    for table in tables {
        let sqlite_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(sqlite)
            .await
            .unwrap_or(0);
        
        let postgres_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
            .fetch_one(postgres)
            .await
            .unwrap_or(0);
        
        if sqlite_count == postgres_count {
            println!("  ✅ {} - SQLite: {}, PostgreSQL: {}", table, sqlite_count, postgres_count);
        } else {
            println!("  ❌ {} - SQLite: {}, PostgreSQL: {} (MISMATCH)", table, sqlite_count, postgres_count);
            all_match = false;
        }
    }
    
    if !all_match {
        println!("❌ Table count validation failed!");
        return Err(anyhow::anyhow!("Table counts don't match"));
    }
    
    Ok(())
}

async fn validate_data_integrity(postgres: &sqlx::PgPool) -> Result<()> {
    println!("\n🔗 Validating data integrity...");
    
    // Check foreign key constraints
    let checks = [
        ("article_entities -> articles", 
         "SELECT COUNT(*) FROM article_entities ae LEFT JOIN articles a ON ae.article_id = a.id WHERE a.id IS NULL"),
        ("article_entities -> entities", 
         "SELECT COUNT(*) FROM article_entities ae LEFT JOIN entities e ON ae.entity_id = e.id WHERE e.id IS NULL"),
        ("entity_aliases -> entities", 
         "SELECT COUNT(*) FROM entity_aliases ea LEFT JOIN entities e ON ea.entity_id = e.id WHERE e.id IS NULL"),
        ("article_cluster_mappings -> articles", 
         "SELECT COUNT(*) FROM article_cluster_mappings acm LEFT JOIN articles a ON acm.article_id = a.id WHERE a.id IS NULL"),
        ("article_cluster_mappings -> clusters", 
         "SELECT COUNT(*) FROM article_cluster_mappings acm LEFT JOIN article_clusters ac ON acm.cluster_id = ac.id WHERE ac.id IS NULL"),
    ];
    
    for (description, query) in checks {
        let orphaned_count: i64 = sqlx::query_scalar(query)
            .fetch_one(postgres)
            .await?;
        
        if orphaned_count == 0 {
            println!("  ✅ {}: No orphaned records", description);
        } else {
            println!("  ❌ {}: Found {} orphaned records", description, orphaned_count);
            return Err(anyhow::anyhow!("Data integrity check failed"));
        }
    }
    
    Ok(())
}

async fn validate_data_samples(sqlite: &sqlx::SqlitePool, postgres: &sqlx::PgPool) -> Result<()> {
    println!("\n🎯 Validating data samples...");
    
    // Sample articles
    let sqlite_articles: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, title FROM articles ORDER BY id LIMIT 5"
    )
    .fetch_all(sqlite)
    .await?;
    
    for (id, title) in sqlite_articles {
        let postgres_title: Option<String> = sqlx::query_scalar(
            "SELECT title FROM articles WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(postgres)
        .await?;
        
        if let Some(pg_title) = postgres_title {
            if title == pg_title {
                println!("  ✅ Article {}: Title matches", id);
            } else {
                println!("  ❌ Article {}: Title mismatch", id);
                return Err(anyhow::anyhow!("Data sample validation failed"));
            }
        } else {
            println!("  ❌ Article {}: Not found in PostgreSQL", id);
            return Err(anyhow::anyhow!("Data sample validation failed"));
        }
    }
    
    Ok(())
}

async fn validate_schema(postgres: &sqlx::PgPool) -> Result<()> {
    println!("\n🏗️  Validating schema...");
    
    // Check essential indexes exist
    let indexes = [
        "idx_relevant_category",
        "idx_hash", 
        "idx_entities_normalized_name",
        "idx_configurations_category"
    ];
    
    for index in indexes {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE indexname = $1)"
        )
        .bind(index)
        .fetch_one(postgres)
        .await?;
        
        if exists {
            println!("  ✅ Index {} exists", index);
        } else {
            println!("  ❌ Index {} missing", index);
            return Err(anyhow::anyhow!("Schema validation failed"));
        }
    }
    
    Ok(())
}
```

## Functional Testing

### Create src/bin/test_postgresql_functionality.rs
```rust
use anyhow::Result;
use argus::{config::ConfigManager, db::Database};

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 Testing PostgreSQL functionality...");
    
    // Test database connection
    test_database_connection().await?;
    
    // Test configuration management
    test_configuration_management().await?;
    
    // Test basic CRUD operations
    test_crud_operations().await?;
    
    println!("✅ All functionality tests passed!");
    Ok(())
}

async fn test_database_connection() -> Result<()> {
    println!("\n🔌 Testing database connection...");
    
    let db = Database::new().await?;
    
    // Test basic query
    match &db.pool {
        argus::db::core::DatabasePool::Postgres(pool) => {
            let version: String = sqlx::query_scalar("SELECT version()")
                .fetch_one(pool)
                .await?;
            println!("  ✅ Connected to PostgreSQL: {}", 
                     version.split_whitespace().take(2).collect::<Vec<_>>().join(" "));
        }
        _ => {
            return Err(anyhow::anyhow!("Expected PostgreSQL connection"));
        }
    }
    
    Ok(())
}

async fn test_configuration_management() -> Result<()> {
    println!("\n⚙️  Testing configuration management...");
    
    let config_manager = ConfigManager::new().await?;
    
    // Test adding and retrieving topic
    config_manager.add_topic("test_topic", "Test topic for validation").await?;
    println!("  ✅ Added test topic");
    
    let topics = config_manager.get_topics().await;
    let test_topic = topics.iter().find(|(name, _)| name == "test_topic");
    if test_topic.is_some() {
        println!("  ✅ Retrieved test topic");
    } else {
        return Err(anyhow::anyhow!("Test topic not found"));
    }
    
    // Test RSS feeds
    let feeds = config_manager.get_rss_feeds().await;
    if !feeds.is_empty() {
        println!("  ✅ Retrieved {} RSS feeds", feeds.len());
    } else {
        println!("  ⚠️  No RSS feeds configured");
    }
    
    // Test system settings
    if let Some(_) = config_manager.get_system_setting("rust_log").await {
        println!("  ✅ Retrieved system settings");
    } else {
        println!("  ⚠️  No system settings found");
    }
    
    Ok(())
}

async fn test_crud_operations() -> Result<()> {
    println!("\n📝 Testing basic CRUD operations...");
    
    let db = Database::new().await?;
    
    // Test configuration CRUD
    db.set_config("test", "crud_test", "test_value").await?;
    println!("  ✅ Created configuration");
    
    let value = db.get_config("test", "crud_test").await?;
    if value == Some("test_value".to_string()) {
        println!("  ✅ Read configuration");
    } else {
        return Err(anyhow::anyhow!("Configuration read failed"));
    }
    
    db.set_config("test", "crud_test", "updated_value").await?;
    let updated_value = db.get_config("test", "crud_test").await?;
    if updated_value == Some("updated_value".to_string()) {
        println!("  ✅ Updated configuration");
    } else {
        return Err(anyhow::anyhow!("Configuration update failed"));
    }
    
    Ok(())
}
```

## Performance Testing

### Create src/bin/test_postgresql_performance.rs
```rust
use anyhow::Result;
use argus::db::Database;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<()> {
    println!("⚡ Testing PostgreSQL performance...");
    
    let db = Database::new().await?;
    
    // Test concurrent configuration access
    test_concurrent_config_access(&db).await?;
    
    // Test query performance
    test_query_performance(&db).await?;
    
    println!("✅ Performance tests completed!");
    Ok(())
}

async fn test_concurrent_config_access(db: &Database) -> Result<()> {
    println!("\n🔄 Testing concurrent configuration access...");
    
    let start = Instant::now();
    
    // Simulate concurrent access
    let tasks = (0..10).map(|i| {
        let db = db.clone();
        tokio::spawn(async move {
            let key = format!("concurrent_test_{}", i);
            let value = format!("value_{}", i);
            
            // Write
            db.set_config("performance_test", &key, &value).await?;
            
            // Read
            let retrieved = db.get_config("performance_test", &key).await?;
            
            if retrieved == Some(value) {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Concurrent access failed for key {}", key))
            }
        })
    });
    
    // Wait for all tasks
    for task in tasks {
        task.await??;
    }
    
    let duration = start.elapsed();
    println!("  ✅ 10 concurrent operations completed in {:?}", duration);
    
    if duration.as_millis() > 5000 {
        println!("  ⚠️  Performance warning: Operations took longer than expected");
    }
    
    Ok(())
}

async fn test_query_performance(db: &Database) -> Result<()> {
    println!("\n📊 Testing query performance...");
    
    match &db.pool {
        argus::db::core::DatabasePool::Postgres(pool) => {
            let start = Instant::now();
            
            // Test large result set
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM articles")
                .fetch_one(pool)
                .await?;
            
            let duration = start.elapsed();
            println!("  ✅ Counted {} articles in {:?}", count, duration);
            
            // Test complex query
            let start = Instant::now();
            let _results: Vec<(i64, String)> = sqlx::query_as(
                "SELECT a.id, a.title FROM articles a 
                 JOIN article_entities ae ON a.id = ae.article_id 
                 WHERE a.is_relevant = true 
                 LIMIT 100"
            )
            .fetch_all(pool)
            .await?;
            
            let duration = start.elapsed();
            println!("  ✅ Complex join query completed in {:?}", duration);
        }
        _ => {
            return Err(anyhow::anyhow!("PostgreSQL pool expected"));
        }
    }
    
    Ok(())
}
```

## Automated Testing Script

### Create test_migration.sh
```bash
#!/bin/bash
set -e

echo "🧪 Starting automated migration testing..."

# Ensure we're using PostgreSQL
if [ "$DATABASE_TYPE" != "postgres" ]; then
    echo "❌ DATABASE_TYPE must be set to 'postgres' for testing"
    exit 1
fi

# Build test binaries
echo "🔨 Building test binaries..."
cargo build --bin validate_migration --bin test_postgresql_functionality --bin test_postgresql_performance

# Run data validation
echo "📊 Running data validation..."
cargo run --bin validate_migration

# Run functionality tests
echo "🧪 Running functionality tests..."
cargo run --bin test_postgresql_functionality

# Run performance tests
echo "⚡ Running performance tests..."
cargo run --bin test_postgresql_performance

# Test configuration validation
echo "⚙️  Testing configuration validation..."
cargo run --bin validate_config

# Test runtime configuration management
echo "🎛️  Testing runtime configuration management..."
cargo run --bin manage_topics list > /dev/null
cargo run --bin manage_feeds list > /dev/null

echo "✅ All tests passed!"
echo ""
echo "Migration validation completed successfully!"
echo "PostgreSQL migration is ready for production deployment."
```

## Integration Testing

### Test Application Startup
```bash
# Test that application starts with PostgreSQL configuration
cargo build --release
DATABASE_TYPE=postgres cargo run --release &
APP_PID=$!

# Wait for startup
sleep 10

# Test basic endpoint (if API exists)
curl -f http://localhost:8080/status || echo "API test failed"

# Cleanup
kill $APP_PID

echo "✅ Application startup test completed"
```

## Validation Checklist
- [ ] All tables migrated with correct row counts
- [ ] Foreign key constraints maintained
- [ ] Data samples match between SQLite and PostgreSQL
- [ ] Essential indexes created
- [ ] Configuration management working
- [ ] CRUD operations functional
- [ ] Concurrent access working
- [ ] Query performance acceptable
- [ ] Application starts successfully
- [ ] Runtime configuration management operational

## Execution Steps
1. **Build test binaries**: `cargo build --bin validate_migration --bin test_postgresql_functionality --bin test_postgresql_performance`
2. **Run automated tests**: `./test_migration.sh`
3. **Review test results**: Ensure all validations pass
4. **Test application**: Start application with PostgreSQL and verify functionality
5. **Performance baseline**: Note query times for future comparison
