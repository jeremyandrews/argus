use anyhow::Result;
use argus::db::Database;
use std::fs;
use tokio::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Migrates the database schema to support enhanced clustering functionality.
///
/// This utility:
/// 1. Drops and recreates the article_clusters table with an enhanced schema
/// 2. Creates article_cluster_mappings table if it doesn't exist
/// 3. Creates user_cluster_preferences table if it doesn't exist
/// 4. Sets up appropriate indexes for all tables
///
/// Usage:
///    cargo run --bin migrate_cluster_schema
///
/// This is a one-time migration that should be run before using clustering features.

#[tokio::main]
pub async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");

    let start_time = Instant::now();
    info!("Starting cluster schema migration...");

    // Read the SQL file
    let sql = fs::read_to_string("migrations/cluster_schema.sql")?;
    info!("Loaded SQL migration file ({} bytes)", sql.len());

    // Get database instance
    let db = Database::instance().await;
    let pool = db.pool();

    // Split the SQL into individual statements
    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Executing {} SQL statements...", statements.len());

    // Execute each statement
    let mut tx = pool.begin().await?;
    for (i, stmt) in statements.iter().enumerate() {
        sqlx::query(stmt).execute(&mut *tx).await?;
        info!("Executed statement {} of {}", i + 1, statements.len());
    }

    // Commit the transaction
    tx.commit().await?;

    // Update our db version
    let version_query = "PRAGMA user_version = 2";
    sqlx::query(version_query).execute(pool).await?;
    info!("Updated database schema version to 2");

    let elapsed = start_time.elapsed();
    info!(
        "Cluster schema migration completed successfully in {:.2?}",
        elapsed
    );

    Ok(())
}
