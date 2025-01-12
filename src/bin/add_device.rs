use anyhow::Result;
use std::env;
use tracing::{error, info};
use tracing_subscriber;

use argus::db::Database;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Display INFO and higher
        .init();

    info!("Starting the add_device helper");

    // Get device ID and topics from command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        error!("Usage: add_device <device_id> [<topic1> <topic2> ...]");
        return Ok(());
    }

    let device_id = &args[1];
    let topics: Vec<String> = if args.len() > 2 {
        args[2..].iter().map(|s| s.to_string()).collect()
    } else {
        // Default topic list
        vec![
            "Alert".to_string(),
            "Apple".to_string(),
            "Bitcoins".to_string(),
            "Clients".to_string(),
            "Drupal".to_string(),
            "E-Ink".to_string(),
            "EVs".to_string(),
            "Global".to_string(),
            "LLMs".to_string(),
            "Longevity".to_string(),
            "Music".to_string(),
            "Rust".to_string(),
            "Space".to_string(),
            "Tuscany".to_string(),
            "Vulnerability".to_string(),
            "Test".to_string(),
        ]
    };

    // Initialize the database
    let db = Database::instance().await;

    // Add the device and subscribe it to topics
    info!("Adding device: {}", device_id);
    for topic in topics {
        match db.subscribe_to_topic(device_id, &topic).await {
            Ok(_) => info!("Subscribed device {} to topic: {}", device_id, topic),
            Err(e) => error!(
                "Failed to subscribe device {} to topic {}: {}",
                device_id, topic, e
            ),
        }
    }

    info!("Finished processing device: {}", device_id);
    Ok(())
}
