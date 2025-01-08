use anyhow::Result;
use serde_json::json;
use tracing::info;
use tracing_subscriber;

use argus::app;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Display INFO and higher
        .init();

    info!("Starting the application");

    let json = json!({
        "topic": "Test",
        "title": "Low priority",
        "url": "http://example.com/1/2/3",
        "tiny_summary": "This is a low priority notification.",
        "tiny_title": "Low Priority Notification",
        "summary": "This is a *long* summary with lots of _detail_.\nIt is more than one line long.\n * One\n * Two\n * Three",
        "critical_analysis": "This is a critical analysis.",
        "logical_fallacies": "This explores logical fallacies.",
        "relation_to_topic": "This is a test.",
        "source_analysis": "This is a test source analysis.",
        "elapsed_time": "12345",
        "model": "test model"
    });
    app::send_to_app(&json, "low").await;

    let json = json!({
        "topic": "Test",
        "title": "High priority",
        "url": "http://example.com/1/2/3",
        "tiny_summary": "This is a high priority notification.",
        "tiny_title": "High Priority Notification",
        "summary": "This is a *long* summary with lots of _detail_.\nIt is more than one line long.\n * One\n * Two\n * Three",
        "critical_analysis": "This is a critical analysis.",
        "logical_fallacies": "This explores logical fallacies.",
        "relation_to_topic": "This is a test.",
        "source_analysis": "This is a test source analysis.",
        "elapsed_time": "12345",
        "model": "test model"
    });
    app::send_to_app(&json, "high").await;
    Ok(())
}
