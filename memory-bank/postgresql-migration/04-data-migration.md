# 04 - Data Migration

**Effort**: M (2 days)

## Export SQLite Data

### Create src/bin/export_sqlite_data.rs
```rust
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, Pool, Sqlite};
use std::fs::{File, create_dir_all};
use std::io::Write;
use anyhow::Result;

#[tokio::main] 
async fn main() -> Result<()> {
    create_dir_all("migration_data")?;
    
    let pool = SqlitePoolOptions::new()
        .connect("sqlite:argus.db")
        .await?;
    
    let tables = [
        "articles", "entities", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings", "cluster_merge_history",
        "rss_queue", "matched_topics_queue", "life_safety_queue",
        "devices", "device_subscriptions"
    ];
    
    for table in tables {
        export_table(&pool, table).await?;
    }
    
    println!("✅ SQLite data exported to CSV files");
    Ok(())
}

async fn export_table(pool: &Pool<Sqlite>, table_name: &str) -> Result<()> {
    let rows = sqlx::query(&format!("SELECT * FROM {}", table_name))
        .fetch_all(pool)
        .await?;
    
    if rows.is_empty() {
        println!("⚠️  Table {} is empty, skipping", table_name);
        return Ok(());
    }
    
    let mut file = File::create(format!("migration_data/{}.csv", table_name))?;
    
    // Write header
    let columns = rows[0].columns();
    let header: Vec<&str> = columns.iter().map(|c| c.name()).collect();
    writeln!(file, "{}", header.join(","))?;
    
    // Write data
    for row in rows {
        let values: Vec<String> = (0..columns.len())
            .map(|i| {
                let value: Option<String> = row.try_get(i).unwrap_or(None);
                match value {
                    Some(v) => format!("\"{}\"", v.replace("\"", "\"\"")),
                    None => "".to_string(),
                }
            })
            .collect();
        writeln!(file, "{}", values.join(","))?;
    }
    
    println!("✅ Exported {} rows from {}", rows.len(), table_name);
    Ok(())
}
```

## Import to PostgreSQL

### Create src/bin/import_postgres_data.rs
```rust
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    // Import in dependency order
    let tables = [
        "entities", "articles", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings", "cluster_merge_history",
        "rss_queue", "matched_topics_queue", "life_safety_queue",
        "devices", "device_subscriptions"
    ];
    
    for table in tables {
        import_csv_via_copy(&pool, table).await?;
    }
    
    // Reset sequences
    reset_sequences(&pool).await?;
    
    println!("✅ PostgreSQL data import completed");
    Ok(())
}

async fn import_csv_via_copy(pool: &sqlx::PgPool, table_name: &str) -> Result<()> {
    let file_path = format!("migration_data/{}.csv", table_name);
    
    if !std::path::Path::new(&file_path).exists() {
        println!("⚠️  File {} not found, skipping {}", file_path, table_name);
        return Ok(());
    }
    
    let mut file = File::open(&file_path)?;
    let mut reader = BufReader::new(&mut file);
    
    // Read header to get column names
    let mut header = String::new();
    reader.read_line(&mut header)?;
    let columns: Vec<&str> = header.trim().split(',').collect();
    
    // Use COPY for efficient bulk import
    let copy_stmt = format!(
        "COPY {} ({}) FROM STDIN WITH (FORMAT CSV, HEADER true)",
        table_name,
        columns.join(", ")
    );
    
    // Reopen file for COPY
    let file = File::open(&file_path)?;
    let mut copy = pool.copy_in_raw(&copy_stmt).await?;
    
    let mut buf = [0; 8192];
    let mut file_reader = std::io::BufReader::new(file);
    
    loop {
        let bytes_read = std::io::Read::read(&mut file_reader, &mut buf)?;
        if bytes_read == 0 {
            break;
        }
        copy.send(&buf[..bytes_read]).await?;
    }
    
    let rows_affected = copy.finish().await?;
    println!("✅ Imported {} rows into {}", rows_affected, table_name);
    Ok(())
}

async fn reset_sequences(pool: &sqlx::PgPool) -> Result<()> {
    let tables = [
        "articles", "entities", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings", "devices"
    ];
    
    for table in tables {
        let _ = sqlx::query(&format!(
            "SELECT setval('{}_id_seq', COALESCE((SELECT MAX(id) FROM {}), 1))",
            table, table
        ))
        .execute(pool)
        .await; // Ignore errors for tables without sequences
    }
    
    println!("✅ Sequences reset");
    Ok(())
}
```

## Data Validation

### Create src/bin/validate_migration.rs
```rust
use sqlx::{sqlite::SqlitePoolOptions, postgres::PgPoolOptions};
use std::env;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let sqlite_pool = SqlitePoolOptions::new()
        .connect("sqlite:argus.db")
        .await?;
    
    let postgres_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    let postgres_pool = PgPoolOptions::new()
        .connect(&postgres_url)
        .await?;
    
    let tables = [
        "articles", "entities", "article_entities", "entity_aliases",
        "article_clusters", "article_cluster_mappings"
    ];
    
    for table in tables {
        validate_table_counts(&sqlite_pool, &postgres_pool, table).await?;
    }
    
    validate_data_integrity(&postgres_pool).await?;
    
    println!("✅ Migration validation completed");
    Ok(())
}

async fn validate_table_counts(
    sqlite: &sqlx::SqlitePool, 
    postgres: &sqlx::PgPool, 
    table: &str
) -> Result<()> {
    let sqlite_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(sqlite)
        .await?;
    
    let postgres_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(postgres)
        .await?;
    
    if sqlite_count == postgres_count {
        println!("✅ {} - SQLite: {}, PostgreSQL: {}", table, sqlite_count, postgres_count);
    } else {
        println!("❌ {} - SQLite: {}, PostgreSQL: {} (MISMATCH)", table, sqlite_count, postgres_count);
    }
    
    Ok(())
}

async fn validate_data_integrity(postgres: &sqlx::PgPool) -> Result<()> {
    // Check foreign key constraints
    let orphaned_article_entities: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM article_entities ae 
         LEFT JOIN articles a ON ae.article_id = a.id 
         WHERE a.id IS NULL"
    )
    .fetch_one(postgres)
    .await?;
    
    if orphaned_article_entities == 0 {
        println!("✅ No orphaned article_entities records");
    } else {
        println!("❌ Found {} orphaned article_entities records", orphaned_article_entities);
    }
    
    // Check entity references
    let orphaned_entity_aliases: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entity_aliases ea
         LEFT JOIN entities e ON ea.entity_id = e.id
         WHERE e.id IS NULL"
    )
    .fetch_one(postgres)
    .await?;
    
    if orphaned_entity_aliases == 0 {
        println!("✅ No orphaned entity_aliases records");
    } else {
        println!("❌ Found {} orphaned entity_aliases records", orphaned_entity_aliases);
    }
    
    Ok(())
}
```

## Migration Script

### Create migrate_data.sh
```bash
#!/bin/bash
set -e

echo "🚀 Starting PostgreSQL data migration..."

# Create migration directory
mkdir -p migration_data

# Export SQLite data
echo "📤 Exporting SQLite data..."
cargo run --bin export_sqlite_data

# Import to PostgreSQL
echo "📥 Importing to PostgreSQL..."
cargo run --bin import_postgres_data

# Validate migration
echo "✅ Validating migration..."
cargo run --bin validate_migration

echo "🎉 Data migration completed successfully!"
```

## Execution Steps
1. Build binaries: `cargo build --bin export_sqlite_data --bin import_postgres_data --bin validate_migration`
2. Run migration: `./migrate_data.sh`
3. Verify results in validation output
4. Test queries on PostgreSQL database
