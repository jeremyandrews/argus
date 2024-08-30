use feed_rs::parser;
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
                        debug!(target: TARGET_WEB_REQUEST, "Request to {} succeeded with status {}", rss_url, response.status());
                        if response.status().is_success() {
                            let body = match response.text().await {
                                Ok(text) => {
                                    debug!(target: TARGET_WEB_REQUEST, "Received body from {}: {}", rss_url, &text);
                                    text
                                }
                                Err(err) => {
                                    error!(target: TARGET_WEB_REQUEST, "Failed to read response body from {}: {}", rss_url, err);
                                    continue;
                                }
                            };

                            // Log the first few characters of the response body for debugging
                            debug!(target: TARGET_WEB_REQUEST, "First 500 characters of response body: {}", &body.chars().take(500).collect::<String>());

                            let reader = io::Cursor::new(body);
                            match parser::parse(reader) {
                                Ok(feed) => {
                                    debug!(target: TARGET_WEB_REQUEST, "Parsed feed with {} entries", feed.entries.len());
                                    for entry in feed.entries {
                                        if let Some(article_url) =
                                            entry.links.first().map(|link| link.href.clone())
                                        {
                                            let article_title =
                                                entry.title.clone().map(|t| t.content);
                                            debug!(target: TARGET_WEB_REQUEST, "Adding article to queue: {}", article_url);
                                            if let Err(err) = db
                                                .add_to_queue(
                                                    &article_url,
                                                    article_title.as_deref(),
                                                )
                                                .await
                                            {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err);
                                            }
                                        } else {
                                            warn!(target: TARGET_WEB_REQUEST, "Feed entry missing link, skipping");
                                        }
                                    }
                                    break true;
                                }
                                Err(err) => {
                                    error!(target: TARGET_WEB_REQUEST, "Failed to parse feed from {}: {}", rss_url, err);
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
        info!(target: TARGET_WEB_REQUEST, "Sleeping for 1 minute before next fetch");
        sleep(Duration::from_secs(60)).await; // Sleep for 1 minute before fetching again
    }
}
