use rss::Channel;
use std::io;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::TARGET_WEB_REQUEST;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRIES: usize = 3;

pub async fn rss_loop(rss_urls: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::instance().await;

    loop {
        for rss_url in &rss_urls {
            // Check if the URL is empty and skip it if so
            if rss_url.trim().is_empty() {
                warn!(target: TARGET_WEB_REQUEST, "Skipping empty RSS URL");
                continue;
            }

            let mut attempts = 0;
            debug!(target: TARGET_WEB_REQUEST, "Starting to process RSS URL: {}", rss_url);

            let success = loop {
                if attempts >= MAX_RETRIES {
                    error!(target: TARGET_WEB_REQUEST, "Max retries reached for URL: {}", rss_url);
                    break false;
                }

                info!(target: TARGET_WEB_REQUEST, "Loading RSS feed from {}", rss_url);
                match timeout(REQUEST_TIMEOUT, reqwest::get(rss_url)).await {
                    Ok(Ok(response)) => {
                        info!(target: TARGET_WEB_REQUEST, "Request to {} succeeded with status {}", rss_url, response.status());
                        if response.status().is_success() {
                            let body = match response.text().await {
                                Ok(text) => text,
                                Err(err) => {
                                    error!(target: TARGET_WEB_REQUEST, "Failed to read response body from {}: {}", rss_url, err);
                                    continue;
                                }
                            };
                            let reader = io::Cursor::new(body);
                            match Channel::read_from(reader) {
                                Ok(channel) => {
                                    info!(target: TARGET_WEB_REQUEST, "Parsed RSS channel with {} items", channel.items().len());
                                    for item in channel.items() {
                                        if let Some(article_url) = item.link.clone() {
                                            debug!(target: TARGET_WEB_REQUEST, "Adding article to queue: {}", article_url);
                                            if let Err(err) = db.add_to_queue(&article_url).await {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err);
                                            }
                                        } else {
                                            warn!(target: TARGET_WEB_REQUEST, "RSS item missing link, skipping");
                                        }
                                    }
                                    break true;
                                }
                                Err(err) => {
                                    error!(target: TARGET_WEB_REQUEST, "Failed to parse RSS channel from {}: {}", rss_url, err);
                                    break false;
                                }
                            }
                        } else {
                            warn!(target: TARGET_WEB_REQUEST, "Non-success status {} from {}", response.status(), rss_url);
                        }
                    }
                    Ok(Err(err)) => {
                        error!(target: TARGET_WEB_REQUEST, "Request to {} failed: {}", rss_url, err);
                    }
                    Err(_) => {
                        error!(target: TARGET_WEB_REQUEST, "Request to {} timed out", rss_url);
                    }
                }

                attempts += 1;
                warn!(target: TARGET_WEB_REQUEST, "Retrying {} in {:?}", rss_url, RETRY_DELAY);
                sleep(RETRY_DELAY).await;
            };

            if !success {
                error!(target: TARGET_WEB_REQUEST, "Failed to process URL: {}", rss_url);
            }
        }
        info!(target: TARGET_WEB_REQUEST, "Sleeping for 1 hour before next fetch");
        sleep(Duration::from_secs(3600)).await; // Sleep for 1 hour before fetching again
    }
}
