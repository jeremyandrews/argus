use crate::db::Database;
use crate::web::{process_item, ProcessItemParams};
use ollama_rs::Ollama;
use serde_json::Value;
use std::collections::BTreeSet;
use tracing::info;

pub async fn worker_loop(
    db: Database,
    topics: &[String],
    ollama: &Ollama,
    model: &str,
    temperature: f32,
    slack_token: &str,
    slack_channel: &str,
    places: Option<Value>,
    non_affected_people: &mut BTreeSet<String>,
    non_affected_places: &mut BTreeSet<String>,
) {
    loop {
        if let Some(url) = db.fetch_and_delete_url_from_queue().await.unwrap() {
            info!("Processing URL: {}", url);
            let item = rss::Item::default(); // Create a default Item or fetch the actual item from the URL if needed

            let mut params = ProcessItemParams {
                topics,
                ollama,
                model,
                temperature,
                db: &db,
                slack_token,
                slack_channel,
                places: places.clone(),
                non_affected_people,
                non_affected_places,
            };

            process_item(item, &mut params).await;
        } else {
            // No more URLs to process, break the loop
            break;
        }
    }
}
