use anyhow::Result;
use feed_rs::parser;
use std::io;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::TARGET_WEB_REQUEST;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRIES: usize = 3;

pub async fn rss_loop(rss_urls: Vec<String>) -> Result<()> {
    let db = Database::instance().await;

    loop {
        if let Err(err) = db.clean_queue().await {
            error!(target: TARGET_WEB_REQUEST, "Failed to clean queue: {}", err);
        }

        // Count and log the number of entries in the queue
        match db.count_queue_entries().await {
            Ok(count) => {
                info!(target: TARGET_WEB_REQUEST, "Processing queue with {} entries", count);
            }
            Err(err) => {
                error!(target: TARGET_WEB_REQUEST, "Failed to count queue entries: {}", err);
            }
        }

        // If a critical unexpected error occurs, catch it and log it here
        if let Err(e) = process_rss_urls(&rss_urls, &db).await {
            error!(target: TARGET_WEB_REQUEST, "Critical failure in rss_loop: {}", e);
            return Err(e.into());
        }

        debug!(target: TARGET_WEB_REQUEST, "Sleeping for 10 minutes before next fetch");
        sleep(Duration::from_secs(600)).await;
    }
}

async fn process_rss_urls(rss_urls: &Vec<String>, db: &Database) -> Result<()> {
    for rss_url in rss_urls {
        // Check if the URL is empty and skip it if so
        if rss_url.trim().is_empty() {
            debug!(target: TARGET_WEB_REQUEST, "Skipping empty RSS URL");
            continue;
        }

        let mut attempts = 0;
        debug!(target: TARGET_WEB_REQUEST, "Starting to process RSS URL: {}", rss_url);

        loop {
            if attempts >= MAX_RETRIES {
                error!(target: TARGET_WEB_REQUEST, "Max retries reached for URL: {}, moving on", rss_url); // Added 'moving on' for clarity
                break; // Move to next RSS URL
            }

            debug!(target: TARGET_WEB_REQUEST, "Loading RSS feed from {}", rss_url);
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
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        };

                        // Log the first few characters of the response body for debugging
                        debug!(target: TARGET_WEB_REQUEST, "First 500 characters of response body: {}", &body.chars().take(500).collect::<String>());
                        let reader = io::Cursor::new(body);
                        match parser::parse(reader) {
                            Ok(feed) => {
                                debug!(target: TARGET_WEB_REQUEST, "Parsed feed with {} entries", feed.entries.len());

                                let mut new_articles_count = 0;

                                for entry in feed.entries {
                                    if let Some(article_url) =
                                        entry.links.first().map(|link| link.href.clone())
                                    {
                                        let article_title = entry.title.clone().map(|t| t.content);
                                        debug!(target: TARGET_WEB_REQUEST, "Adding article to queue: {}", article_url);

                                        match db
                                            .add_to_queue(&article_url, article_title.as_deref())
                                            .await
                                        {
                                            Ok(added) => {
                                                if added {
                                                    new_articles_count += 1;
                                                }
                                            }
                                            Err(err) => {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err);
                                            }
                                        }
                                    } else {
                                        debug!(target: TARGET_WEB_REQUEST, "Feed entry missing link, skipping");
                                    }
                                }

                                if new_articles_count > 0 {
                                    info!(target: TARGET_WEB_REQUEST, "Added {} new articles from {}", new_articles_count, rss_url);
                                } else {
                                    debug!(target: TARGET_WEB_REQUEST, "No new articles added from {}", rss_url);
                                }
                                break; // Successfully processed, move on to next URL
                            }
                            Err(err) => {
                                error!(target: TARGET_WEB_REQUEST, "Failed to parse feed from {}: {}", rss_url, err);
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        }
                    } else {
                        warn!(target: TARGET_WEB_REQUEST, "Non-success status {} from {}", response.status(), rss_url);
                        attempts += 1;
                        sleep(RETRY_DELAY).await;
                        continue;
                    }
                }
                Ok(Err(err)) => {
                    error!(target: TARGET_WEB_REQUEST, "Request to {} failed: {}", rss_url, err);
                    attempts += 1;
                    sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(_) => {
                    error!(target: TARGET_WEB_REQUEST, "Request to {} timed out", rss_url);
                    attempts += 1;
                    sleep(RETRY_DELAY).await;
                    continue;
                }
            }
        }
    }

    Ok(())
}
