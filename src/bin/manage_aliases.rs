use anyhow::{anyhow, Context, Result};
use argus::db::Database;
use argus::entity::aliases;
use argus::entity::normalizer::EntityNormalizer;
use argus::entity::types::EntityType;
use clap::{Parser, Subcommand};
use tokio::main;
use tracing::info;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Migrate static aliases to the database
    Migrate,

    /// Add a new alias to the system
    Add {
        /// Canonical entity name
        #[arg(short, long)]
        canonical: String,

        /// Alias text to add
        #[arg(short, long)]
        alias: String,

        /// Entity type (person, organization, product, location)
        #[arg(short, long)]
        entity_type: String,

        /// Source of the alias (admin, pattern, llm)
        #[arg(short, long, default_value = "admin")]
        source: String,

        /// Confidence score (0.0-1.0)
        #[arg(short, long, default_value = "1.0")]
        confidence: f64,
    },

    /// Test if two entity names match
    Test {
        /// First entity name
        #[arg(short, long)]
        name1: String,

        /// Second entity name
        #[arg(short, long)]
        name2: String,

        /// Entity type (person, organization, product, location)
        #[arg(short, long)]
        entity_type: String,
    },

    /// Create a batch of aliases for review
    CreateReviewBatch {
        /// Number of aliases to include in the batch
        #[arg(short, long, default_value = "20")]
        size: i64,
    },

    /// Review a specific batch of aliases
    ReviewBatch {
        /// Batch ID to review
        #[arg(short, long)]
        batch_id: i64,

        /// Admin ID for tracking who approved/rejected
        #[arg(short, long, default_value = "cli-user")]
        admin_id: String,
    },

    /// Display alias system statistics
    Stats,
}

#[main]
async fn main() -> Result<()> {
    // Initialize tracing
    argus::logging::configure_logging();

    let cli = Cli::parse();

    // Get database connection
    let database_url = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());
    let db = Database::new(&database_url)
        .await
        .context("Failed to connect to database")?;

    match cli.command {
        Commands::Migrate => {
            info!("Migrating static aliases to database...");
            let count = db.migrate_static_aliases().await?;
            println!("Successfully migrated {} static aliases to database", count);
        }

        Commands::Add {
            canonical,
            alias,
            entity_type,
            source,
            confidence,
        } => {
            let entity_type = parse_entity_type(&entity_type)?;

            info!(
                "Adding new alias: {} ↔ {} ({})",
                canonical, alias, entity_type
            );
            let alias_id = aliases::add_alias(
                &db,
                None,
                &canonical,
                &alias,
                entity_type,
                &source,
                confidence,
            )
            .await?;

            if alias_id > 0 {
                println!("Successfully added alias with ID: {}", alias_id);
            } else {
                println!("Alias not added (may be duplicate or identical normalized form)");
            }
        }

        Commands::Test {
            name1,
            name2,
            entity_type,
        } => {
            let entity_type = parse_entity_type(&entity_type)?;
            let normalizer = EntityNormalizer::new();

            // Test with both methods for comparison
            println!(
                "Testing if '{}' matches '{}' as {} entities:",
                name1, name2, entity_type
            );

            // 1. Synchronous in-memory method
            let sync_result = normalizer.names_match(&name1, &name2, entity_type);
            println!("  - In-memory alias match: {}", sync_result);

            // 2. Database-backed method
            let async_result = normalizer
                .async_names_match(&db, &name1, &name2, entity_type)
                .await?;
            println!("  - Database-backed match: {}", async_result);

            // Show normalizer output for better understanding
            let norm1 = normalizer.normalize(&name1, entity_type);
            let norm2 = normalizer.normalize(&name2, entity_type);
            println!("  - Normalized form of '{}': '{}'", name1, norm1);
            println!("  - Normalized form of '{}': '{}'", name2, norm2);
        }

        Commands::CreateReviewBatch { size } => {
            info!("Creating review batch with size {}", size);
            let batch_id = db.create_alias_review_batch(size).await?;
            println!(
                "Created review batch #{} with up to {} items",
                batch_id, size
            );
        }

        Commands::ReviewBatch { batch_id, admin_id } => {
            info!("Reviewing batch #{}", batch_id);
            let aliases = db.get_alias_review_batch(batch_id).await?;

            if aliases.is_empty() {
                println!(
                    "No aliases found in batch #{} (may be empty or already reviewed)",
                    batch_id
                );
                return Ok(());
            }

            println!(
                "Found {} aliases to review in batch #{}",
                aliases.len(),
                batch_id
            );

            for (idx, (alias_id, canonical, alias_text, entity_type, source, confidence)) in
                aliases.iter().enumerate()
            {
                println!(
                    "\nReview {}/{}: {} ↔ {} ({})",
                    idx + 1,
                    aliases.len(),
                    canonical,
                    alias_text,
                    entity_type
                );
                println!("Source: {}, Confidence: {:.2}", source, confidence);

                println!("Approve (a), Reject (r), or Skip (s)? ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;

                match input.trim().to_lowercase().as_str() {
                    "a" => {
                        db.approve_alias_suggestion(*alias_id, &admin_id).await?;
                        println!("Approved alias #{}", alias_id);
                    }
                    "r" => {
                        println!(
                            "Rejection reason? (1) Not an alias, (2) Different entity, (3) Other"
                        );
                        let mut reason_input = String::new();
                        std::io::stdin().read_line(&mut reason_input)?;

                        let reason = match reason_input.trim() {
                            "1" => Some("not an alias"),
                            "2" => Some("different entity"),
                            _ => Some("other"),
                        };

                        db.reject_alias_suggestion(*alias_id, &admin_id, reason)
                            .await?;
                        println!("Rejected alias #{}", alias_id);
                    }
                    _ => {
                        println!("Skipped alias #{}", alias_id);
                    }
                }
            }

            println!("\nCompleted review of batch #{}", batch_id);
        }

        Commands::Stats => {
            info!("Retrieving alias system statistics");
            let stats = db.get_alias_system_stats().await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
    }

    Ok(())
}

/// Parse entity type string into EntityType enum
fn parse_entity_type(entity_type: &str) -> Result<EntityType> {
    match entity_type.to_lowercase().as_str() {
        "person" => Ok(EntityType::Person),
        "organization" | "org" => Ok(EntityType::Organization),
        "product" => Ok(EntityType::Product),
        "location" => Ok(EntityType::Location),
        "event" => Ok(EntityType::Event),
        _ => Err(anyhow!("Invalid entity type: {}. Must be one of: person, organization, product, location, event", entity_type)),
    }
}
