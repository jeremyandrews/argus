use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use rand::{rngs::StdRng, Rng, SeedableRng};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::db::core::Database;
use crate::util::parse_places_data_hierarchical;
use crate::workers::common::{build_connection_info, ProcessItemParams};
use crate::{LLMClient, WorkerDetail, TARGET_LLM_REQUEST};

use super::processing::process_item;

/// Main decision worker loop that continuously processes items from the RSS queue
pub async fn decision_loop(
    worker_id: i16,
    topics: &[String],
    llm_client: &LLMClient,
    model: &str,
    temperature: f32,
    slack_token: &str,
    slack_channel: &str,
    no_think: bool,
    thinking_config: Option<crate::ThinkingModelConfig>,
) -> Result<()> {
    let db = Database::instance().await;
    let mut rng = StdRng::seed_from_u64(rand::random());

    // Extract connection info from the LLM client
    let connection_info = build_connection_info(llm_client, worker_id, "DECISION_OLLAMA_CONFIGS");

    let worker_detail = WorkerDetail {
        name: "decision worker".to_string(),
        id: worker_id,
        model: model.to_string(),
        connection_info,
    };

    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: starting decision_loop using {:?}.", worker_detail.name, worker_detail.id, worker_detail.model, llm_client);

    // Each decision_worker loads places before entering the main loop.
    let places = match parse_places_data_hierarchical() {
        Ok(hierarchy) => hierarchy,
        Err(err) => panic!("Error: {}", err),
    };

    loop {
        // Determine which article to select next: 30% of the time select the newest
        // (latest news), 25% oldest (stale queue), 45% random.
        let roll = rng.random_range(0..=99); // Updated to avoid deprecation warning
        let order = if roll < 30 {
            "newest"
        } else if roll < 55 {
            "oldest"
        } else {
            "random"
        };

        match db.fetch_and_delete_url_from_rss_queue(order).await {
            Ok(Some((url, title, pub_date))) => {
                if url.trim().is_empty() {
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping empty URL in queue.", worker_detail.name, worker_detail.id, worker_detail.model);
                    continue;
                }

                // Parse pub_date and check if it's older than 3 days
                let is_old_article = if let Some(date_str) = &pub_date {
                    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                        .ok()
                        .map(|date| {
                            let now = Utc::now().date_naive();
                            now.signed_duration_since(date) > ChronoDuration::days(3)
                            // Use ChronoDuration here
                        })
                        .unwrap_or(false)
                } else {
                    false
                };

                if is_old_article {
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping old article (published on {:?}): {}.", worker_detail.name, worker_detail.id, worker_detail.model, pub_date, url);

                    // Store it in the database to prevent reprocessing
                    let _ = db
                        .add_article(
                            &url,
                            false, // Not relevant
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            pub_date.as_deref(),
                            None, // event_date
                        )
                        .await;
                    continue;
                }

                info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: loaded URL: {} ({:?}).", worker_detail.name, worker_detail.id, worker_detail.model, url, title);

                let item = crate::workers::common::FeedItem {
                    url: url.clone(),
                    title,
                    pub_date,
                };

                let places_clone = places.clone();

                let mut params = ProcessItemParams {
                    topics,
                    llm_client,
                    model,
                    temperature,
                    db: &db,
                    slack_token,
                    slack_channel,
                    places: places_clone,
                    thinking_config: thinking_config.clone(),
                    no_think,
                };

                process_item(item, &mut params, &worker_detail).await;
            }
            Ok(None) => {
                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no URLs in queue, sleeping for 1 minute.", worker_detail.name, worker_detail.id, worker_detail.model);
                sleep(Duration::from_secs(60)).await;
                continue;
            }
            Err(e) => {
                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error fetching URL from queue ({:?}), sleeping for 5 seconds.", worker_detail.name, worker_detail.id, worker_detail.model, e);
                sleep(Duration::from_secs(5)).await; // Wait and retry
                continue;
            }
        }
    }
}
