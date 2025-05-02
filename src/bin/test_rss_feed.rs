use anyhow::Result;
use argus::db::Database;
use argus::logging;
use argus::rss;
use colored::Colorize;
use std::env;
use std::process;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    logging::configure_logging();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage(&args[0]);
        return Ok(());
    }

    let url = &args[1];
    let add_to_queue = args.iter().any(|arg| arg == "--add-to-queue");

    println!("Testing RSS feed: {}", url);
    if add_to_queue {
        println!("Will add entries to article queue if feed is valid");
    }

    // Initialize database connection if adding to queue
    let db = if add_to_queue {
        Some(Database::instance().await)
    } else {
        None
    };

    // Call the test_rss_feed function from the rss module
    match rss::test_rss_feed(url, db.as_deref()).await {
        Ok(result) => {
            println!("\n{}", "═".repeat(100).bright_blue());
            println!(
                "{}  {}",
                "FEED DIAGNOSTICS".bright_blue(),
                url.bright_yellow()
            );
            println!("{}", "═".repeat(100).bright_blue());

            // Print status with appropriate color
            let status_str = format!("{:?}", result.status);
            let colored_status = match result.status {
                rss::RssFeedStatus::Success => status_str.bright_green(),
                rss::RssFeedStatus::RequestFailed | rss::RssFeedStatus::RequestTimeout => {
                    status_str.bright_red()
                }
                _ => status_str.bright_yellow(),
            };
            println!("{}: {}", "Status".bright_blue(), colored_status);

            // Print Content-Type if available
            if let Some(ref content_type) = result.content_type {
                println!("{}: {}", "Content-Type".bright_blue(), content_type);
            } else {
                println!("{}: {}", "Content-Type".bright_blue(), "None".dimmed());
            }

            // Print detected encoding if available
            if let Some(ref encoding) = result.detected_encoding {
                println!("{}: {}", "Detected Encoding".bright_blue(), encoding);
            } else {
                println!(
                    "{}: {}",
                    "Detected Encoding".bright_blue(),
                    "Unknown".dimmed()
                );
            }

            // Print HTTP headers if any
            if !result.headers.is_empty() {
                println!("\n{}", "HTTP Headers".bright_blue());
                println!("{}", "─".repeat(80).dimmed());
                for (name, value) in &result.headers {
                    println!("{}: {}", name.bright_magenta(), value);
                }
            }

            // Print entries found
            println!(
                "{}: {}",
                "Entries Found".bright_blue(),
                result.entries_found
            );

            // Print raw preview if available
            if let Some(ref raw_preview) = result.raw_preview {
                println!("\n{}", "Raw Content Preview (hex)".bright_blue());
                println!("{}", "─".repeat(80).dimmed());

                // Format as hex dump with ASCII representation
                for chunk in raw_preview.chunks(16) {
                    let hex_values: Vec<String> =
                        chunk.iter().map(|b| format!("{:02x}", b)).collect();

                    // Create ASCII representation
                    let ascii: String = chunk
                        .iter()
                        .map(|&b| if b >= 32 && b <= 126 { b as char } else { '.' })
                        .collect();

                    println!(
                        "{}  {}",
                        hex_values.join(" ").dimmed(),
                        ascii.bright_white()
                    );
                }
            }

            // Print decoded preview if available
            if let Some(ref decoded) = result.decoded_preview {
                println!("\n{}", "Decoded Content Preview".bright_blue());
                println!("{}", "─".repeat(80).dimmed());
                println!("{}", decoded);
            }

            // Print warnings if any
            if !result.warnings.is_empty() {
                println!("\n{}", "Warnings".bright_yellow());
                println!("{}", "─".repeat(80).dimmed());
                for (i, warning) in result.warnings.iter().enumerate() {
                    println!("{}. {}", i + 1, warning);
                }
            }

            // Print errors if any
            if !result.errors.is_empty() {
                println!("\n{}", "Errors".bright_red());
                println!("{}", "─".repeat(80).dimmed());
                for (i, error) in result.errors.iter().enumerate() {
                    println!("{}. {}", i + 1, error.bright_red());
                }
            }

            // Print entry details if available and successful
            if !result.entries.is_empty() {
                println!("\n{}", "Feed Entries".bright_green());
                println!("{}", "─".repeat(80).dimmed());

                // Limit to 5 entries to avoid overwhelming output
                let display_count = std::cmp::min(result.entries.len(), 5);
                for (i, entry) in result.entries.iter().take(display_count).enumerate() {
                    let title = entry.title.as_deref().unwrap_or("[No Title]");
                    let url = entry.url.as_deref().unwrap_or("[No URL]");
                    let pub_date = match &entry.pub_date {
                        Some(date) => date.as_str(),
                        None => "[No Date]",
                    };

                    println!(
                        "{}. {} ({})\n   {}",
                        i + 1,
                        title.bright_white(),
                        pub_date.dimmed(),
                        url.bright_cyan()
                    );
                }

                if result.entries.len() > 5 {
                    println!("... and {} more entries", result.entries.len() - 5);
                }
            }

            println!("\n{}", "═".repeat(100).bright_blue());

            // Return error code if feed had problems
            match result.status {
                rss::RssFeedStatus::Success => {
                    println!(
                        "Feed test completed successfully with {} entries found",
                        result.entries_found
                    );
                    process::exit(0);
                }
                _ => {
                    eprintln!("Feed test completed with errors: {:?}", result.status);
                    process::exit(1);
                }
            }
        }
        Err(err) => {
            eprintln!("Failed to test feed: {}", err);
            process::exit(2);
        }
    }
}

// Print usage instructions
fn print_usage(program_name: &str) {
    println!("Usage: {} <rss_url> [--add-to-queue]", program_name);
    println!("\nOptions:");
    println!("  --add-to-queue    Add parsed entries to the article queue");
    println!("\nExamples:");
    println!("  {} https://www.example.com/feed", program_name);
    println!(
        "  {} https://www.example.com/feed --add-to-queue",
        program_name
    );
}
