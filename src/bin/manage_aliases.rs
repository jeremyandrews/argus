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
        #[arg(short = 'n', long)]
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
        #[arg(short = '1', long)]
        name1: String,

        /// Second entity name
        #[arg(short = '2', long)]
        name2: String,

        /// Entity type (person, organization, product, location)
        #[arg(short, long)]
        entity_type: String,
    },

    /// Review pending aliases
    Review {
        /// Number of aliases to review (default: 20)
        #[arg(short, long, default_value = "20")]
        limit: i64,

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

        Commands::Review { limit, admin_id } => {
            info!("Starting alias review with limit {}", limit);
            let aliases = db.get_pending_aliases(limit).await?;

            if aliases.is_empty() {
                println!("No pending aliases found to review.");
                return Ok(());
            }

            println!("Found {} pending aliases to review.", aliases.len());
            println!("(Use 'q' to quit at any time)");

            let mut reviewed_count = 0;
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

                print!("Approve (a), Reject (r), Skip (s), or Quit (q)? ");
                use std::io::{self, Write};
                io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;

                match input.trim().to_lowercase().as_str() {
                    "a" => {
                        db.approve_alias_suggestion(*alias_id, &admin_id).await?;
                        println!("✅ Approved alias #{}", alias_id);
                        reviewed_count += 1;
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
                        println!("❌ Rejected alias #{}", alias_id);
                        reviewed_count += 1;
                    }
                    "q" => {
                        println!("\n🛑 Review cancelled by user.");
                        break;
                    }
                    _ => {
                        println!("⏭️ Skipped alias #{}", alias_id);
                    }
                }
            }

            println!(
                "\n📋 Review completed! Processed {} out of {} aliases.",
                reviewed_count,
                aliases.len()
            );
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
