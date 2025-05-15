//! Feed parsing logic for RSS, Atom, and JSON formats.

use anyhow::Result;
use chrono::{self, Duration as ChronoDuration, Utc};
use feed_rs::parser;
use serde_json;
use std::io::{self, Cursor};
use tracing::{debug, error};

use super::types::{EntryInfo, JsonFeed, RssFeedStatus, TestRssFeedResult};
use super::util::{add_entries_to_database, cleanup_xml, parse_date};
use crate::db::core::Database;
use crate::TARGET_WEB_REQUEST;

/// Process the feed content after extraction and decompression
pub async fn process_feed_content(
    body: String,
    mut result: TestRssFeedResult,
    content_type: Option<String>,
    db: Option<&Database>,
    url: &str,
) -> Result<TestRssFeedResult> {
    // Try to parse as JSON or XML based on content type
    if let Some(ref ct) = content_type {
        if ct.contains("json") {
            match serde_json::from_str::<JsonFeed>(&body) {
                Ok(feed) => {
                    result.entries_found = feed.items.len();

                    // Extract entry information
                    for item in feed.items {
                        if let Some(article_url) = item.url.or(item.id) {
                            let entry = EntryInfo {
                                title: item.title,
                                url: Some(article_url),
                                pub_date: item.date_published,
                            };
                            result.entries.push(entry);
                        }
                    }

                    // Optional: Add to database if db is provided
                    if let Some(db) = db {
                        add_entries_to_database(&result.entries, db, url).await;
                    }

                    return Ok(result);
                }
                Err(err) => {
                    result.status = RssFeedStatus::ParseError;
                    result
                        .errors
                        .push(format!("Failed to parse JSON feed: {}", err));
                    return Ok(result);
                }
            }
        }
    }

    // Try to parse as XML (RSS/Atom)
    let reader = io::Cursor::new(&body);
    match parser::parse(reader) {
        Ok(feed) => {
            result.entries_found = feed.entries.len();

            // Extract entry information
            for entry in feed.entries {
                let entry_info = EntryInfo {
                    title: entry.title.map(|t| t.content),
                    url: entry.links.first().map(|link| link.href.clone()),
                    pub_date: entry.published.map(|d| d.to_rfc3339()),
                };
                result.entries.push(entry_info);
            }

            // Optional: Add to database if db is provided
            if let Some(db) = db {
                add_entries_to_database(&result.entries, db, url).await;
            }

            return Ok(result);
        }
        Err(first_err) => {
            // Try cleaning the XML first
            let cleaned_xml = cleanup_xml(&body);

            // Check if it looks like RSS/Atom
            if cleaned_xml.contains("<rss") || cleaned_xml.contains("<feed") {
                let reader = Cursor::new(cleaned_xml);
                match parser::parse(reader) {
                    Ok(feed) => {
                        result
                            .warnings
                            .push("Feed parsed successfully after XML cleanup".to_string());
                        result.entries_found = feed.entries.len();

                        // Extract entry information
                        for entry in feed.entries {
                            let entry_info = EntryInfo {
                                title: entry.title.map(|t| t.content),
                                url: entry.links.first().map(|link| link.href.clone()),
                                pub_date: entry.published.map(|d| d.to_rfc3339()),
                            };
                            result.entries.push(entry_info);
                        }

                        // Optional: Add to database if db is provided
                        if let Some(db) = db {
                            add_entries_to_database(&result.entries, db, url).await;
                        }

                        return Ok(result);
                    }
                    Err(second_err) => {
                        result.status = RssFeedStatus::ParseError;
                        result.errors.push(format!("Failed to parse feed even after cleanup. First error: {}. Second error: {}", first_err, second_err));
                        return Ok(result);
                    }
                }
            } else {
                result.status = RssFeedStatus::NotRssOrAtom;
                let preview = if body
                    .chars()
                    .all(|c| c.is_ascii_graphic() || c.is_whitespace())
                {
                    body.chars().take(100).collect::<String>()
                } else {
                    "[binary data]".to_string()
                };
                result.errors.push(format!(
                    "Feed doesn't appear to be RSS or Atom. Content preview: {}",
                    preview
                ));
                return Ok(result);
            }
        }
    }
}

/// Process different types of feeds and add entries to the queue
pub async fn process_feed(
    text: &str,
    content_type: Option<&str>,
    db: &Database,
    rss_url: &str,
    _browser_emulation_used: bool,
) -> Result<usize> {
    let mut new_articles_count = 0;

    // Process different formats based on content type
    if let Some(ct) = content_type {
        if ct.contains("json") {
            // Parse as JSON feed
            debug!(target: TARGET_WEB_REQUEST, "Processing as JSON feed: {}", rss_url);
            match serde_json::from_str::<JsonFeed>(text) {
                Ok(feed) => {
                    for item in feed.items {
                        if let Some(article_url) = item.url.or(item.id) {
                            let pub_date = item.date_published.map(|d| {
                                if let Some(dt) = parse_date(&d) {
                                    dt.to_rfc3339()
                                } else {
                                    d
                                }
                            });

                            if let Ok(added) = process_article_entry(
                                &article_url,
                                item.title.as_deref(),
                                pub_date.as_deref(),
                                db,
                            )
                            .await
                            {
                                if added {
                                    new_articles_count += 1;
                                }
                            }
                        }
                    }
                    return Ok(new_articles_count);
                }
                Err(err) => {
                    error!(target: TARGET_WEB_REQUEST, "Failed to parse JSON feed from {}: {}", rss_url, err);
                    return Err(anyhow::anyhow!("JSON parsing error: {}", err));
                }
            }
        }
    }

    // Parse as XML (RSS/Atom)
    debug!(target: TARGET_WEB_REQUEST, "Processing as XML feed: {}", rss_url);
    let reader = io::Cursor::new(text);
    match parser::parse(reader) {
        Ok(feed) => {
            for entry in feed.entries {
                if let Some(article_url) = entry.links.first().map(|link| link.href.clone()) {
                    let article_title = entry.title.map(|t| t.content);
                    let pub_date = entry.published.map(|d| d.to_rfc3339());

                    if let Ok(added) = process_article_entry(
                        &article_url,
                        article_title.as_deref(),
                        pub_date.as_deref(),
                        db,
                    )
                    .await
                    {
                        if added {
                            new_articles_count += 1;
                        }
                    }
                }
            }

            return Ok(new_articles_count);
        }
        Err(first_err) => {
            // Try cleaning the XML first
            let cleaned_xml = cleanup_xml(text);

            // Check if it looks like RSS/Atom
            if cleaned_xml.contains("<rss") || cleaned_xml.contains("<feed") {
                let reader = io::Cursor::new(&cleaned_xml);
                match parser::parse(reader) {
                    Ok(feed) => {
                        for entry in feed.entries {
                            if let Some(article_url) =
                                entry.links.first().map(|link| link.href.clone())
                            {
                                let article_title = entry.title.map(|t| t.content);
                                let pub_date = entry.published.map(|d| d.to_rfc3339());

                                if let Ok(added) = process_article_entry(
                                    &article_url,
                                    article_title.as_deref(),
                                    pub_date.as_deref(),
                                    db,
                                )
                                .await
                                {
                                    if added {
                                        new_articles_count += 1;
                                    }
                                }
                            }
                        }
                        return Ok(new_articles_count);
                    }
                    Err(second_err) => {
                        error!(
                            target: TARGET_WEB_REQUEST,
                            "Failed to parse feed from {} after cleanup. First error: {}. Second error: {}",
                            rss_url,
                            first_err,
                            second_err
                        );
                        return Err(anyhow::anyhow!("XML parsing error even after cleanup"));
                    }
                }
            } else {
                let preview = if text
                    .chars()
                    .all(|c| c.is_ascii_graphic() || c.is_whitespace())
                {
                    text.chars().take(100).collect::<String>()
                } else {
                    "[binary data]".to_string()
                };
                error!(
                    target: TARGET_WEB_REQUEST,
                    "Feed from {} doesn't appear to be RSS or Atom. Content preview: {}",
                    rss_url,
                    preview
                );
                return Err(anyhow::anyhow!("Content is not RSS or Atom feed"));
            }
        }
    }
}

/// Process an individual article entry, checking age and adding to queue if appropriate
async fn process_article_entry(
    article_url: &str,
    article_title: Option<&str>,
    pub_date: Option<&str>,
    db: &Database,
) -> Result<bool> {
    // Check if article is too old (>1 week)
    let is_old = if let Some(pub_date_str) = pub_date {
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
        if let Err(err) = db
            .add_article(
                article_url,
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                pub_date,
                None,
            )
            .await
        {
            error!(target: TARGET_WEB_REQUEST, "Failed to log old article: {}", err);
        }
        return Ok(false);
    }

    match db.add_to_queue(article_url, article_title, pub_date).await {
        Ok(true) => {
            debug!(target: TARGET_WEB_REQUEST, "Added article to queue: {}", article_url);
            Ok(true)
        }
        Ok(false) => {
            debug!(target: TARGET_WEB_REQUEST, "Article already in queue or processed: {}", article_url);
            Ok(false)
        }
        Err(err) => {
            error!(target: TARGET_WEB_REQUEST, "Failed to add article to queue: {}", err);
            Err(anyhow::anyhow!("Failed to add to queue: {}", err))
        }
    }
}
