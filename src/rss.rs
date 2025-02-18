use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use encoding_rs;
use feed_rs::parser;
use reqwest::cookie::Jar;
use std::io;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use url;

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

fn cleanup_xml(xml: &str) -> String {
    let mut cleaned = xml
        .replace("&nbsp;", "&#160;")
        .replace("&ndash;", "&#8211;")
        .replace("&mdash;", "&#8212;")
        .replace("&rsquo;", "&#8217;");

    // Remove any invalid XML characters
    cleaned = cleaned
        .chars()
        .filter(|&c| {
            matches!(c, '\u{0009}' | '\u{000A}' | '\u{000D}' | '\u{0020}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}')
        })
        .collect();

    cleaned
}

fn parse_date(date_str: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339
    if let Ok(date) = DateTime::parse_from_rfc3339(date_str) {
        return Some(date.with_timezone(&Utc));
    }

    // Try RFC2822
    if let Ok(date) = DateTime::parse_from_rfc2822(date_str) {
        return Some(date.with_timezone(&Utc));
    }

    None
}

fn is_valid_url(url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.scheme() == "http" || parsed.scheme() == "https"
    } else {
        false
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
            match timeout(REQUEST_TIMEOUT, client.get(rss_url)
                .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .header(reqwest::header::ACCEPT, "application/rss+xml, application/atom+xml, application/xml, text/xml, application/json, */*;q=0.9")
                .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate, br")
                .send()).await
            {
                Ok(Ok(response)) => {
                    debug!(target: TARGET_WEB_REQUEST, "Response Content-Type: {:?}", response.headers().get(reqwest::header::CONTENT_TYPE));

                    // Get content type before consuming response
                    let content_type = response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|ct| ct.to_str().ok())
                        .map(|s| s.to_string());

                    if response.status().is_success() {
                        let body = match response.text().await {
                            Ok(text) => {
                                if text.starts_with("<?xml") {
                                    text
                                } else {
                                    // Try to detect encoding from saved content-type header
                                    if let Some(ct_str) = content_type {
                                        if let Some(charset) = ct_str.split(';')
                                            .find(|part| part.trim().to_lowercase().starts_with("charset="))
                                            .and_then(|charset| charset.split('=').nth(1))
                                        {
                                            if let Some(encoding) = encoding_rs::Encoding::for_label(charset.trim().as_bytes()) {
                                                let (decoded, _, _) = encoding.decode(text.as_bytes());
                                                decoded.into_owned()
                                            } else {
                                                text
                                            }
                                        } else {
                                            text
                                        }
                                    } else {
                                        text
                                    }
                                }
                            }
                            Err(err) => {
                                error!(target: TARGET_WEB_REQUEST, "Failed to read response body from {}: {}", rss_url, err);
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        };

                        debug!(target: TARGET_WEB_REQUEST, "Fetched body: {}", body);

                        let reader = io::Cursor::new(&body);
                        match parser::parse(reader) {
                            Ok(feed) => {
                                for entry in feed.entries {
                                    if let Some(article_url) = entry.links.first().map(|link| link.href.clone()) {
                                        let article_title = entry.title.clone().map(|t| t.content);
                                        let pub_date = entry.published.map(|d| d.to_rfc3339());

                                        let is_old = if let Some(pub_date_str) = &pub_date {
                                            if let Some(pub_date_dt) = parse_date(pub_date_str) {
                                                Utc::now().signed_duration_since(pub_date_dt) > ChronoDuration::weeks(1)
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
                            Err(first_err) => {
                                // Try cleaning the XML first
                                let cleaned_xml = cleanup_xml(&body);
                                let reader = io::Cursor::new(cleaned_xml);
                                match parser::parse(reader) {
                                    Ok(feed) => {
                                        // Same feed processing code as above
                                        for entry in feed.entries {
                                            if let Some(article_url) = entry.links.first().map(|link| link.href.clone()) {
                                                let article_title = entry.title.clone().map(|t| t.content);
                                                let pub_date = entry.published.map(|d| d.to_rfc3339());

                                                let is_old = if let Some(pub_date_str) = &pub_date {
                                                    if let Some(pub_date_dt) = parse_date(pub_date_str) {
                                                        Utc::now().signed_duration_since(pub_date_dt) > ChronoDuration::weeks(1)
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
                                        break;
                                    }
                                    Err(_) => {
                                        error!(target: TARGET_WEB_REQUEST, "Failed to parse feed from {}: {}", rss_url, first_err);
                                        attempts += 1;
                                        sleep(RETRY_DELAY).await;
                                        continue;
                                    }
                                }
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
