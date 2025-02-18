use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use feed_rs::parser;
use reqwest::cookie::Jar;
use std::io;
use std::sync::Arc;
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

async fn process_rss_urls(rss_urls: &Vec<String>, db: &Database) -> Result<()> {
    let cookie_store = Jar::default();
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::new(cookie_store))
        .gzip(true)
        .redirect(reqwest::redirect::Policy::default())
        .build()?;

    for rss_url in rss_urls {
        if rss_url.trim().is_empty() {
            debug!(target: TARGET_WEB_REQUEST, "Skipping empty RSS URL");
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
            match timeout(REQUEST_TIMEOUT, client.get(rss_url)
                .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .header(reqwest::header::ACCEPT, "application/rss+xml, application/xml, text/xml")
                .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate, br")
                .send()).await
            {
                Ok(Ok(response)) => {
                    debug!(target: TARGET_WEB_REQUEST, "Response Content-Type: {:?}", response.headers().get(reqwest::header::CONTENT_TYPE));

                    if let Some(ct) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                        if ct.to_str()?.contains("json") {
                            error!(target: TARGET_WEB_REQUEST, "Received JSON instead of RSS from {}", rss_url);
                            attempts += 1;
                            sleep(RETRY_DELAY).await;
                            continue;
                        }
                    }

                    if response.status().is_success() {
                        let body = match response.text().await {
                            Ok(text) => text,
                            Err(err) => {
                                error!(target: TARGET_WEB_REQUEST, "Failed to read response body from {}: {}", rss_url, err);
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        };

                        debug!(target: TARGET_WEB_REQUEST, "Fetched body: {}", body);

                        let reader = io::Cursor::new(body);
                        match parser::parse(reader) {
                            Ok(feed) => {
                                for entry in feed.entries {
                                    if let Some(article_url) = entry.links.first().map(|link| link.href.clone()) {
                                        let article_title = entry.title.clone().map(|t| t.content);
                                        let pub_date = entry.published.map(|d| d.to_rfc3339());

                                        let is_old = if let Some(pub_date_str) = &pub_date {
                                            if let Ok(pub_date_dt) = DateTime::parse_from_rfc3339(pub_date_str) {
                                                let pub_date_utc = pub_date_dt.with_timezone(&Utc);
                                                Utc::now().signed_duration_since(pub_date_utc) > ChronoDuration::weeks(1)
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        };

                                        if is_old {
                                            debug!(target: TARGET_WEB_REQUEST, "Skipping analysis for old article: {} ({:?})", article_url, pub_date);
                                            if let Err(err) = db.add_article(&article_url, false, None, None, None, None, None, None, pub_date.as_deref()).await {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to log old article: {}", err);
                                            }
                                            continue;
                                        }

                                        match db.add_to_queue(&article_url, article_title.as_deref(), pub_date.as_deref()).await {
                                            Ok(true) => {
                                                new_articles_count += 1;
                                                debug!(target: TARGET_WEB_REQUEST, "Added article to queue: {}", article_url);
                                            }
                                            Ok(false) => {
                                                debug!(target: TARGET_WEB_REQUEST, "Article already in queue or processed: {}", article_url);
                                            }
                                            Err(err) => {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err);
                                            }
                                        }
                                    }
                                }

                                if new_articles_count > 0 {
                                    info!(target: TARGET_WEB_REQUEST, "Processed RSS feed: {} - {} new articles added", rss_url, new_articles_count);
                                } else {
                                    debug!(target: TARGET_WEB_REQUEST, "Processed RSS feed: {} - No new articles added", rss_url);
                                }

                                break;
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
