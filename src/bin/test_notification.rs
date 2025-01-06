use anyhow::Result;
use serde_json::json;

use argus::app;

#[tokio::main]
async fn main() -> Result<()> {
    let json = json!({
        "topic": "test",
        "title": "A test article title.",
        "url": "http://example.com/1/2/3",
        "tiny_summary": "A tiny summary.",
        "summary": "This is a *long* summary with lots of _detail_.\nIt is more than one line long.\n * One\n * Two\n * Three",
        "critical_analysis": "This is a critical analysis.",
        "logical_fallacies": "This explores logical fallacies.",
        "relation_to_topic": "This is a test.",
        "source_analysis": "This is a test source analysis.",
        "elapsed_time": "12345",
        "model": "test model"
    });
    app::send_to_app(&json).await;
    Ok(())
}
