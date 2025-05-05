use anyhow::Result;
use argus::db::core::Database;
use std::fs;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    argus::logging::setup_logging("migrate_cluster_merge_schema", Level::INFO)?;
    info!("Starting cluster merge schema migration");

    // Load the SQL file
    let sql = fs::read_to_string("migrations/cluster_merge_schema.sql")?;

    // Execute the SQL statements
    let db = Database::instance().await;
    let result = sqlx::query(&sql).execute(db.pool()).await?;

    info!(
        "Migration completed. Rows affected: {}",
        result.rows_affected()
    );

    Ok(())
}
