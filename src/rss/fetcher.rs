//! Main RSS fetching functionality for Argus.

use anyhow::Result;
use reqwest::header;
use std::io::Read;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use super::client::fetch_with_fallback;
use super::parser::process_feed;
use super::types::{MAX_RETRIES, RETRY_DELAY};
use super::util::{is_valid_url, try_decompressions};
use crate::db::core::Database;
use crate::TARGET_WEB_REQUEST;

/// Main RSS fetching loop - periodically fetches all configured RSS feeds
pub async fn rss_loop(rss_urls: Vec<String>) -> Result<()> {
    let db = Database::instance().await;

    loop {
        if let Err(err) = db.clean_queue().await {
            error!(target: TARGET_WEB_REQUEST, "Failed to clean queue: {}", err);
        }

        match db.count_queue_entries().await {
            Ok(count) => {
                info!(target: TARGET_WEB_REQUEST, "Processing queue with {} entries", count);
            }
            Err(err) => {
                error!(target: TARGET_WEB_REQUEST, "Failed to count queue entries: {}", err);
            }
        }

        if let Err(e) = process_rss_urls(&rss_urls, &db).await {
            error!(target: TARGET_WEB_REQUEST, "Critical failure in rss_loop: {}", e);
            return Err(e.into());
        }

        debug!(target: TARGET_WEB_REQUEST, "Sleeping for 10 minutes before next fetch");
        sleep(Duration::from_secs(600)).await;
    }
}

/// Process a list of RSS URLs
pub async fn process_rss_urls(rss_urls: &Vec<String>, db: &Database) -> Result<()> {
    for rss_url in rss_urls {
        if rss_url.trim().is_empty() {
            debug!(target: TARGET_WEB_REQUEST, "Skipping empty RSS URL");
            continue;
        }

        if !is_valid_url(rss_url) {
            debug!(target: TARGET_WEB_REQUEST, "Skipping invalid URL: {}", rss_url);
            continue;
        }

        let mut attempts = 0;
        let mut new_articles_count = 0;

        debug!(target: TARGET_WEB_REQUEST, "Starting to process RSS URL: {}", rss_url);

        loop {
            if attempts >= MAX_RETRIES {
                error!(target: TARGET_WEB_REQUEST, "Max retries reached for URL: {}, moving on", rss_url);
                break;
            }

            debug!(target: TARGET_WEB_REQUEST, "Loading RSS feed from {}", rss_url);

            // Use the fetch_with_fallback function to attempt the request
            match fetch_with_fallback(rss_url).await {
                Ok((response, browser_emulation_used)) => {
                    // Log if browser emulation was used
                    if browser_emulation_used {
                        info!(target: TARGET_WEB_REQUEST, "Browser emulation was required for {}", rss_url);
                    }

                    debug!(target: TARGET_WEB_REQUEST, "Response Content-Type: {:?}", 
                           response.headers().get(header::CONTENT_TYPE));

                    let content_type = response
                        .headers()
                        .get(header::CONTENT_TYPE)
                        .and_then(|ct| ct.to_str().ok())
                        .map(|s| s.to_lowercase());

                    if response.status().is_success() {
                        // Extract the content encoding before consuming the response
                        let content_encoding = response
                            .headers()
                            .get(header::CONTENT_ENCODING)
                            .and_then(|value| value.to_str().ok())
                            .map(|s| s.to_lowercase());

                        // Get the raw bytes next (this consumes the response)
                        let bytes = match response.bytes().await {
                            Ok(b) => b,
                            Err(err) => {
                                error!(target: TARGET_WEB_REQUEST,
                                       "Failed to read response bytes from {}: {}", rss_url, err);
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        };

                        // Try different decompression methods
                        let decompressed_bytes = if content_encoding.as_deref() == Some("br") {
                            let mut decoded = Vec::new();
                            let mut reader = brotli::Decompressor::new(&bytes[..], 4096);
                            if reader.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                                debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed brotli content from {}", rss_url);
                                decoded
                            } else {
                                debug!(target: TARGET_WEB_REQUEST, "Brotli decompression failed for {}, trying other methods", rss_url);
                                try_decompressions(&bytes, rss_url)
                            }
                        } else {
                            try_decompressions(&bytes, rss_url)
                        };

                        // Convert to UTF-8 string
                        match String::from_utf8(decompressed_bytes.clone()) {
                            Ok(text) => {
                                if text.starts_with("<?xml")
                                    || text.contains("<rss")
                                    || text.contains("<feed")
                                {
                                    debug!(target: TARGET_WEB_REQUEST, "Found XML markers in decompressed data from {}", rss_url);
                                }

                                match process_feed(
                                    &text,
                                    content_type.as_deref(),
                                    db,
                                    rss_url,
                                    browser_emulation_used,
                                )
                                .await
                                {
                                    Ok(count) => {
                                        new_articles_count += count;

                                        if count > 0 {
                                            info!(target: TARGET_WEB_REQUEST, "Processed RSS feed: {} - {} new articles added", rss_url, count);
                                        } else {
                                            debug!(target: TARGET_WEB_REQUEST, "Processed RSS feed: {} - No new articles added", rss_url);
                                        }

                                        break;
                                    }
                                    Err(e) => {
                                        error!(target: TARGET_WEB_REQUEST, "Error processing feed {}: {}", rss_url, e);
                                        attempts += 1;
                                        sleep(RETRY_DELAY).await;
                                        continue;
                                    }
                                }
                            }
                            Err(_) => {
                                error!(target: TARGET_WEB_REQUEST, "Failed to decode content as UTF-8 from {}", rss_url);
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
                Err(err) => {
                    error!(target: TARGET_WEB_REQUEST, "Request to {} failed: {}", rss_url, err);
                    attempts += 1;
                    sleep(RETRY_DELAY).await;
                    continue;
                }
            }
        }

        // Log the total number of new articles
        if new_articles_count > 0 {
            info!(target: TARGET_WEB_REQUEST, "Total new articles added from {}: {}", rss_url, new_articles_count);
        }
    }
    Ok(())
}
