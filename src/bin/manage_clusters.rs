use anyhow::Result;
use argus::clustering;
use argus::db::core::Database;
use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use prettytable::{Cell, Row as PrettyRow, Table};
use sqlx::Row;
use std::io::{self, Write};
use tracing::Level;

#[derive(Parser)]
#[clap(name = "cluster-manager", about = "Manage article clusters")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all clusters or a specific cluster
    List {
        /// Filter by cluster ID
        #[clap(short, long)]
        id: Option<i64>,

        /// Number of clusters to show
        #[clap(short, long, default_value = "10")]
        limit: i64,

        /// Include merged clusters
        #[clap(short, long)]
        include_merged: bool,

        /// Sort by importance (default is by last updated)
        #[clap(short, long)]
        by_importance: bool,
    },

    /// Show details about a specific cluster
    Show {
        /// Cluster ID
        #[clap(required = true)]
        id: i64,

        /// Show articles in this cluster
        #[clap(short, long)]
        articles: bool,

        /// Number of articles to show
        #[clap(short, long, default_value = "5")]
        limit: i64,
    },

    /// Find clusters that could be merged based on entity overlap
    FindMergeCandidates {
        /// Minimum entity overlap ratio (0.0-1.0)
        #[clap(short, long, default_value = "0.6")]
        threshold: f64,
    },

    /// Merge multiple clusters into a new one
    Merge {
        /// IDs of clusters to merge (at least 2)
        #[clap(required = true, num_args = 2..)]
        cluster_ids: Vec<i64>,

        /// Reason for the merge (optional)
        #[clap(short, long)]
        reason: Option<String>,
    },

    /// Force regenerate summary for a cluster
    RegenerateSummary {
        /// Cluster ID
        #[clap(required = true)]
        id: i64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    argus::logging::setup_logging("manage_clusters", Level::INFO)?;

    // Parse command line arguments
    let args = Cli::parse();

    // Initialize database connection
    let db = Database::instance().await;

    // Execute the appropriate command
    match args.command {
        Commands::List {
            id,
            limit,
            include_merged,
            by_importance,
        } => {
            list_clusters(&db, id, limit, include_merged, by_importance).await?;
        }
        Commands::Show {
            id,
            articles,
            limit,
        } => {
            show_cluster(&db, id, articles, limit).await?;
        }
        Commands::FindMergeCandidates { threshold } => {
            find_merge_candidates(&db, threshold).await?;
        }
        Commands::Merge {
            cluster_ids,
            reason,
        } => {
            merge_clusters(&db, &cluster_ids, reason.as_deref()).await?;
        }
        Commands::RegenerateSummary { id } => {
            regenerate_summary(&db, id).await?;
        }
    }

    Ok(())
}

/// Lists clusters in a formatted table
async fn list_clusters(
    db: &Database,
    id: Option<i64>,
    limit: i64,
    include_merged: bool,
    by_importance: bool,
) -> Result<()> {
    // Build query based on parameters
    let order_by = if by_importance {
        "importance_score DESC"
    } else {
        "last_updated DESC"
    };

    let status_filter = if include_merged {
        ""
    } else {
        "WHERE status = 'active'"
    };

    let id_filter = if let Some(cluster_id) = id {
        format!(
            "{} id = {}",
            if status_filter.is_empty() {
                "WHERE"
            } else {
                "AND"
            },
            cluster_id
        )
    } else {
        String::new()
    };

    let query = format!(
        r#"
        SELECT id, creation_date, last_updated, article_count, 
               importance_score, status, 
               SUBSTR(summary, 1, 100) as summary_preview
        FROM article_clusters
        {} {}
        ORDER BY {}
        LIMIT ?
        "#,
        status_filter, id_filter, order_by
    );

    let rows = sqlx::query(&query).bind(limit).fetch_all(db.pool()).await?;

    // Create table
    let mut table = Table::new();
    table.add_row(PrettyRow::new(vec![
        Cell::new("ID"),
        Cell::new("Created"),
        Cell::new("Updated"),
        Cell::new("Articles"),
        Cell::new("Importance"),
        Cell::new("Status"),
        Cell::new("Summary Preview"),
    ]));

    for row in rows {
        let id: i64 = row.get("id");
        let created: String = row.get("creation_date");
        let updated: String = row.get("last_updated");
        let article_count: i32 = row.get("article_count");
        let importance: f64 = row.get("importance_score");
        let status: String = row.get("status");
        let summary_preview: Option<String> = row.get("summary_preview");

        // Format dates for readability
        let created_date = DateTime::parse_from_rfc3339(&created)
            .map(|dt| {
                dt.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or(created);

        let updated_date = DateTime::parse_from_rfc3339(&updated)
            .map(|dt| {
                dt.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or(updated);

        table.add_row(PrettyRow::new(vec![
            Cell::new(&id.to_string()),
            Cell::new(&created_date),
            Cell::new(&updated_date),
            Cell::new(&article_count.to_string()),
            Cell::new(&format!("{:.2}", importance)),
            Cell::new(&status),
            Cell::new(&summary_preview.unwrap_or_else(|| "No summary".to_string())),
        ]));
    }

    table.printstd();
    Ok(())
}

/// Shows detailed information about a specific cluster
async fn show_cluster(
    db: &Database,
    cluster_id: i64,
    show_articles: bool,
    limit: i64,
) -> Result<()> {
    // Get basic cluster info
    let row = sqlx::query(
        r#"
        SELECT id, creation_date, last_updated, primary_entity_ids,
               summary, summary_version, article_count, 
               importance_score, has_timeline, status
        FROM article_clusters
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_optional(db.pool())
    .await?;

    let row = match row {
        Some(r) => r,
        None => {
            println!("❌ Cluster {} not found", cluster_id);
            return Ok(());
        }
    };

    let id: i64 = row.get("id");
    let created: String = row.get("creation_date");
    let updated: String = row.get("last_updated");
    let primary_entity_ids: String = row.get("primary_entity_ids");
    let summary: Option<String> = row.get("summary");
    let summary_version: i32 = row.get("summary_version");
    let article_count: i32 = row.get("article_count");
    let importance: f64 = row.get("importance_score");
    let has_timeline: i32 = row.get("has_timeline");
    let status: String = row.get("status");

    // Format dates for readability
    let created_date = DateTime::parse_from_rfc3339(&created)
        .map(|dt| {
            dt.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or(created);

    let updated_date = DateTime::parse_from_rfc3339(&updated)
        .map(|dt| {
            dt.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or(updated);

    // Print cluster details
    println!("=== CLUSTER #{} ===", id);
    println!("Status: {}", status);
    println!("Created: {}", created_date);
    println!("Updated: {}", updated_date);
    println!("Articles: {}", article_count);
    println!("Importance: {:.2}", importance);
    println!("Summary Version: {}", summary_version);
    println!(
        "Has Timeline: {}",
        if has_timeline == 1 { "Yes" } else { "No" }
    );

    if status == "merged" {
        // Check if this cluster was merged into another
        let merge_info = sqlx::query(
            r#"
            SELECT merged_into_cluster_id, merge_date, merge_reason
            FROM cluster_merge_history
            WHERE original_cluster_id = ?
            "#,
        )
        .bind(cluster_id)
        .fetch_optional(db.pool())
        .await?;

        if let Some(merge_row) = merge_info {
            let merged_into: i64 = merge_row.get("merged_into_cluster_id");
            let merge_date: String = merge_row.get("merge_date");
            let merge_reason: Option<String> = merge_row.get("merge_reason");

            println!("\n⚠️ MERGED CLUSTER");
            println!("This cluster has been merged into cluster #{}", merged_into);
            println!("Merge Date: {}", merge_date);

            if let Some(reason) = merge_reason {
                println!("Reason: {}", reason);
            }

            println!("\nTo see the current active cluster:");
            println!("  cargo run --bin manage_clusters -- show {}", merged_into);
        }
    }

    // Get entity details
    let entity_ids: Vec<i64> = serde_json::from_str(&primary_entity_ids)?;
    println!("\n=== PRIMARY ENTITIES ({}) ===", entity_ids.len());

    if !entity_ids.is_empty() {
        // Create a placeholder string for SQL query
        let placeholders = (0..entity_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            r#"
            SELECT id, canonical_name, entity_type
            FROM entities
            WHERE id IN ({})
            "#,
            placeholders
        );

        let mut q = sqlx::query(&query);
        for id in &entity_ids {
            q = q.bind(id);
        }

        let entity_rows = q.fetch_all(db.pool()).await?;

        let mut entity_table = Table::new();
        entity_table.add_row(PrettyRow::new(vec![
            Cell::new("ID"),
            Cell::new("Name"),
            Cell::new("Type"),
        ]));

        for row in entity_rows {
            let id: i64 = row.get("id");
            let name: String = row.get("canonical_name");
            let etype: String = row.get("entity_type");

            entity_table.add_row(PrettyRow::new(vec![
                Cell::new(&id.to_string()),
                Cell::new(&name),
                Cell::new(&etype),
            ]));
        }

        entity_table.printstd();
    } else {
        println!("No primary entities for this cluster");
    }

    // Check if other clusters were merged into this one
    let merged_in = sqlx::query(
        r#"
        SELECT original_cluster_id, merge_date, merge_reason
        FROM cluster_merge_history
        WHERE merged_into_cluster_id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_all(db.pool())
    .await?;

    if !merged_in.is_empty() {
        println!("\n=== CLUSTERS MERGED INTO THIS ONE ===");
        let mut merge_table = Table::new();
        merge_table.add_row(PrettyRow::new(vec![
            Cell::new("Original ID"),
            Cell::new("Merge Date"),
            Cell::new("Reason"),
        ]));

        for row in merged_in {
            let original_id: i64 = row.get("original_cluster_id");
            let merge_date: String = row.get("merge_date");
            let merge_reason: Option<String> = row.get("merge_reason");

            merge_table.add_row(PrettyRow::new(vec![
                Cell::new(&original_id.to_string()),
                Cell::new(&merge_date),
                Cell::new(&merge_reason.unwrap_or_else(|| "".to_string())),
            ]));
        }

        merge_table.printstd();
    }

    // Print summary
    println!("\n=== SUMMARY ===");
    println!(
        "{}",
        summary.unwrap_or_else(|| "No summary available".to_string())
    );

    // Show articles if requested
    if show_articles {
        println!("\n=== ARTICLES (top {}) ===", limit);

        let article_rows = sqlx::query(
            r#"
            SELECT a.id, a.title, a.url, a.pub_date, a.source, acm.similarity_score
            FROM articles a
            JOIN article_cluster_mappings acm ON a.id = acm.article_id
            WHERE acm.cluster_id = ?
            ORDER BY a.pub_date DESC, acm.similarity_score DESC
            LIMIT ?
            "#,
        )
        .bind(cluster_id)
        .bind(limit)
        .fetch_all(db.pool())
        .await?;

        let mut article_table = Table::new();
        article_table.add_row(PrettyRow::new(vec![
            Cell::new("ID"),
            Cell::new("Date"),
            Cell::new("Source"),
            Cell::new("Similarity"),
            Cell::new("Title"),
        ]));

        for row in article_rows {
            let id: i64 = row.get("id");
            let title: Option<String> = row.get("title");
            let pub_date: Option<String> = row.get("pub_date");
            let source: Option<String> = row.get("source");
            let similarity: f64 = row.get("similarity_score");

            // Format date for readability
            let date = pub_date
                .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
                .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            article_table.add_row(PrettyRow::new(vec![
                Cell::new(&id.to_string()),
                Cell::new(&date),
                Cell::new(&source.unwrap_or_else(|| "Unknown".to_string())),
                Cell::new(&format!("{:.2}", similarity)),
                Cell::new(&title.unwrap_or_else(|| "Untitled".to_string())),
            ]));
        }

        article_table.printstd();
    }

    Ok(())
}

/// Finds clusters that could be merged based on entity overlap
async fn find_merge_candidates(db: &Database, threshold: f64) -> Result<()> {
    println!(
        "Finding merge candidates with minimum overlap ratio of {:.2}...",
        threshold
    );

    let candidates = clustering::find_clusters_with_entity_overlap(db, threshold).await?;

    if candidates.is_empty() {
        println!("No merge candidates found matching the criteria.");
        return Ok(());
    }

    println!("Found {} potential merge groups:", candidates.len());

    for (i, group) in candidates.iter().enumerate() {
        println!("\nGroup #{}: {} clusters", i + 1, group.len());

        // Get basic info for each cluster in this group
        for &cluster_id in group {
            let row = sqlx::query(
                r#"
                SELECT id, article_count, 
                       SUBSTR(summary, 1, 100) as summary_preview
                FROM article_clusters
                WHERE id = ?
                "#,
            )
            .bind(cluster_id)
            .fetch_one(db.pool())
            .await?;

            let id: i64 = row.get("id");
            let article_count: i32 = row.get("article_count");
            let summary_preview: Option<String> = row.get("summary_preview");

            println!(
                "  Cluster #{} ({} articles): {}",
                id,
                article_count,
                summary_preview.unwrap_or_else(|| "No summary".to_string())
            );
        }

        println!("\n  To merge these clusters, run:");
        println!(
            "  cargo run --bin manage_clusters -- merge {}",
            group
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    Ok(())
}

/// Merges multiple clusters into a new one
async fn merge_clusters(db: &Database, cluster_ids: &[i64], reason: Option<&str>) -> Result<()> {
    if cluster_ids.len() < 2 {
        println!("❌ At least two clusters are required for merging");
        return Ok(());
    }

    println!("Merging clusters: {:?}", cluster_ids);

    // Confirm with user before proceeding
    print!("⚠️ This operation cannot be undone. Continue? [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Operation cancelled");
        return Ok(());
    }

    // Perform the merge
    match clustering::merge_clusters(db, cluster_ids, reason).await {
        Ok(new_cluster_id) => {
            println!(
                "✅ Successfully merged clusters into new cluster #{}",
                new_cluster_id
            );

            // Show the new cluster
            show_cluster(db, new_cluster_id, true, 5).await?;
        }
        Err(e) => {
            println!("❌ Error merging clusters: {}", e);
        }
    }

    Ok(())
}

/// Regenerates the summary for a specific cluster
async fn regenerate_summary(db: &Database, cluster_id: i64) -> Result<()> {
    println!("Regenerating summary for cluster #{}...", cluster_id);

    // Check if cluster exists and is active
    let cluster = sqlx::query(
        r#"
        SELECT status FROM article_clusters WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .fetch_optional(db.pool())
    .await?;

    match cluster {
        None => {
            println!("❌ Cluster {} not found", cluster_id);
            return Ok(());
        }
        Some(row) => {
            let status: String = row.get("status");
            if status == "merged" {
                println!(
                    "⚠️ Cluster {} has been merged and is no longer active",
                    cluster_id
                );
                print!("Are you sure you want to regenerate its summary? [y/N] ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Operation cancelled");
                    return Ok(());
                }
            }
        }
    }

    // Mark the cluster as needing a summary update
    sqlx::query(
        r#"
        UPDATE article_clusters
        SET needs_summary_update = 1
        WHERE id = ?
        "#,
    )
    .bind(cluster_id)
    .execute(db.pool())
    .await?;

    // Generate the summary
    match clustering::generate_cluster_summary(
        db,
        &argus::vector::get_default_llm_client(),
        cluster_id,
    )
    .await
    {
        Ok(summary) => {
            println!("✅ Successfully regenerated summary:");
            println!("\n{}\n", summary);
        }
        Err(e) => {
            println!("❌ Error generating summary: {}", e);
        }
    }

    Ok(())
}
