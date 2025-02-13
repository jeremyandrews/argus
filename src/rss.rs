use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
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
    for rss_url in rss_urls {
        if rss_url.trim().is_empty() {
            debug!(target: TARGET_WEB_REQUEST, "Skipping empty RSS URL");
            continue;
        }

        let mut attempts = 0;
        let mut new_articles_count: usize;
        debug!(target: TARGET_WEB_REQUEST, "Starting to process RSS URL: {}", rss_url);

        loop {
            if attempts >= MAX_RETRIES {
                error!(target: TARGET_WEB_REQUEST, "Max retries reached for URL: {}, moving on", rss_url);
                break;
            }

            debug!(target: TARGET_WEB_REQUEST, "Loading RSS feed from {}", rss_url);
            match timeout(REQUEST_TIMEOUT, reqwest::get(rss_url)).await {
                Ok(Ok(response)) => {
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

                        let reader = io::Cursor::new(body);
                        match parser::parse(reader) {
                            Ok(feed) => {
                                new_articles_count = 0;
                                for entry in feed.entries {
                                    if let Some(article_url) =
                                        entry.links.first().map(|link| link.href.clone())
                                    {
                                        let article_title = entry.title.clone().map(|t| t.content);
                                        let pub_date = entry.published.map(|d| d.to_rfc3339());

                                        let is_old = if let Some(pub_date_str) = &pub_date {
                                            if let Ok(pub_date_dt) =
                                                DateTime::parse_from_rfc3339(pub_date_str)
                                            {
                                                let pub_date_utc = pub_date_dt.with_timezone(&Utc);
                                                Utc::now().signed_duration_since(pub_date_utc)
                                                    > ChronoDuration::weeks(1)
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        };

                                        if is_old {
                                            debug!(target: TARGET_WEB_REQUEST, "Skipping analysis for old article: {} ({:?})", article_url, pub_date);
                                            match db
                                                .add_article(
                                                    &article_url,
                                                    false,
                                                    None,
                                                    None,
                                                    None,
                                                    None,
                                                    None,
                                                    None,
                                                    pub_date.as_deref(),
                                                )
                                                .await
                                            {
                                                Ok(_) => {
                                                    debug!(target: TARGET_WEB_REQUEST, "Logged old article in database: {}", article_url)
                                                }
                                                Err(err) => {
                                                    error!(target: TARGET_WEB_REQUEST, "Failed to log old article: {}", err)
                                                }
                                            }
                                            continue;
                                        }

                                        match db
                                            .add_to_queue(
                                                &article_url,
                                                article_title.as_deref(),
                                                pub_date.as_deref(),
                                            )
                                            .await
                                        {
                                            Ok(true) => {
                                                new_articles_count += 1;
                                                debug!(target: TARGET_WEB_REQUEST, "Added article to queue: {}", article_url);
                                            }
                                            Ok(false) => {
                                                debug!(target: TARGET_WEB_REQUEST, "Article already in queue or processed: {}", article_url);
                                            }
                                            Err(err) => {
                                                error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err)
                                            }
                                        }
                                    }
                                }

                                if new_articles_count > 0 {
                                    info!(target: TARGET_WEB_REQUEST, "Added {} new articles from {}", new_articles_count, rss_url);
                                }
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
