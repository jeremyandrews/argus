use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use encoding_rs;
use feed_rs::parser;
use flate2;
use reqwest::cookie::Jar;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use url;

use crate::db::Database;
use crate::TARGET_WEB_REQUEST;

// Diagnostic structures for RSS feed testing
#[derive(Debug, Clone, Serialize)]
pub enum RssFeedStatus {
    Success,
    InvalidEncoding,
    NotRssOrAtom,
    RequestFailed,
    ParseError,
    RequestTimeout,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestRssFeedResult {
    pub status: RssFeedStatus,
    pub content_type: Option<String>,
    pub raw_preview: Option<Vec<u8>>,
    pub decoded_preview: Option<String>,
    pub entries_found: usize,
    pub detected_encoding: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub entries: Vec<EntryInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryInfo {
    pub title: Option<String>,
    pub url: Option<String>,
    pub pub_date: Option<String>,
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_RETRIES: usize = 3;

#[derive(Debug, Deserialize)]
struct JsonFeed {
    #[serde(default)]
    items: Vec<JsonFeedItem>,
}

#[derive(Debug, Deserialize)]
struct JsonFeedItem {
    id: Option<String>,
    url: Option<String>,
    title: Option<String>,
    date_published: Option<String>,
}

fn cleanup_xml(xml: &str) -> String {
    let mut cleaned = xml.trim().to_string();

    // Remove any UTF-8 BOM if present
    if cleaned.starts_with('\u{FEFF}') {
        cleaned = cleaned[3..].to_string();
    }

    // Remove any leading whitespace or invalid characters before <?xml or <rss
    if let Some(xml_start) = cleaned.find("<?xml") {
        cleaned = cleaned[xml_start..].to_string();
    } else if let Some(rss_start) = cleaned.find("<rss") {
        cleaned = cleaned[rss_start..].to_string();
    } else if let Some(feed_start) = cleaned.find("<feed") {
        cleaned = cleaned[feed_start..].to_string();
    }

    // Replace common problematic entities
    cleaned = cleaned
        .replace("&nbsp;", "&#160;")
        .replace("&ndash;", "&#8211;")
        .replace("&mdash;", "&#8212;")
        .replace("&rsquo;", "&#8217;")
        .replace("&lsquo;", "&#8216;")
        .replace("&rdquo;", "&#8221;")
        .replace("&ldquo;", "&#8220;")
        .replace("&amp;amp;", "&amp;")
        .replace("&apos;", "&#39;");

    // Remove any invalid XML characters
    cleaned = cleaned
        .chars()
        .filter(|&c| {
            matches!(c,
                '\u{0009}' | // tab
                '\u{000A}' | // newline
                '\u{000D}' | // carriage return
                '\u{0020}'..='\u{D7FF}' |
                '\u{E000}'..='\u{FFFD}' |
                '\u{10000}'..='\u{10FFFF}'
            )
        })
        .collect();

    // Ensure proper XML declaration if missing
    if !cleaned.starts_with("<?xml") {
        cleaned = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", cleaned);
    }

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

    // Try ISO 8601
    if let Ok(date) = DateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S%z") {
        return Some(date.with_timezone(&Utc));
    }

    // Try common formats
    for format in &[
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y",
    ] {
        if let Ok(date) = DateTime::parse_from_str(date_str, format) {
            return Some(date.with_timezone(&Utc));
        }
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

// Function to test a single RSS feed with detailed diagnostics
pub async fn test_rss_feed(url: &str, db: Option<&Database>) -> Result<TestRssFeedResult> {
    let mut result = TestRssFeedResult {
        status: RssFeedStatus::Success,
        content_type: None,
        raw_preview: None,
        decoded_preview: None,
        entries_found: 0,
        detected_encoding: None,
        errors: Vec::new(),
        warnings: Vec::new(),
        entries: Vec::new(),
    };

    // Validate URL
    if !is_valid_url(url) {
        result.status = RssFeedStatus::RequestFailed;
        result.errors.push(format!("Invalid URL format: {}", url));
        return Ok(result);
    }

    // Create HTTP client
    let cookie_store = Jar::default();
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::new(cookie_store))
        .gzip(true)
        .redirect(reqwest::redirect::Policy::default())
        .build()?;

    // Attempt to fetch the RSS feed
    let response = match timeout(REQUEST_TIMEOUT, client.get(url)
        .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header(reqwest::header::ACCEPT, "application/feed+json, application/json, application/rss+xml, application/atom+xml, application/xml, text/xml, */*;q=0.9")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate, br")
        .send()).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(err)) => {
            result.status = RssFeedStatus::RequestFailed;
            result.errors.push(format!("Request failed: {}", err));
            return Ok(result);
        },
        Err(_) => {
            result.status = RssFeedStatus::RequestTimeout;
            result.errors.push(format!("Request timed out after {} seconds", REQUEST_TIMEOUT.as_secs()));
            return Ok(result);
        }
    };

    // Process response status
    if !response.status().is_success() {
        result.status = RssFeedStatus::RequestFailed;
        result
            .errors
            .push(format!("HTTP error: {}", response.status()));
        return Ok(result);
    }

    // Extract content type
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .map(|s| s.to_lowercase());

    result.content_type = content_type.clone();

    // Get the raw bytes
    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(err) => {
            result.status = RssFeedStatus::RequestFailed;
            result
                .errors
                .push(format!("Failed to read response bytes: {}", err));
            return Ok(result);
        }
    };

    // Store the first 100 bytes as a preview
    let preview_size = 100.min(bytes.len());
    result.raw_preview = Some(bytes[..preview_size].to_vec());

    // Try different decompression methods until one works
    let decompressed_bytes = {
        // First try gzip
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut decoded = Vec::new();
        if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
            result
                .warnings
                .push("Content was gzip compressed".to_string());
            decoded
        } else {
            // Try zlib
            let mut decoder = flate2::read::ZlibDecoder::new(&bytes[..]);
            let mut decoded = Vec::new();
            if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                result
                    .warnings
                    .push("Content was zlib compressed".to_string());
                decoded
            } else {
                // Try deflate
                let mut decoder = flate2::read::DeflateDecoder::new(&bytes[..]);
                let mut decoded = Vec::new();
                if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                    result
                        .warnings
                        .push("Content was deflate compressed".to_string());
                    decoded
                } else {
                    // If no decompression worked, use original bytes
                    result.warnings.push("No compression detected".to_string());
                    bytes.to_vec()
                }
            }
        }
    };

    // Try to convert to UTF-8 string
    let body = match String::from_utf8(decompressed_bytes.clone()) {
        Ok(text) => {
            // Store decoded preview
            let preview_size = 200.min(text.len());
            result.decoded_preview = Some(text[..preview_size].to_string());

            if text.starts_with("<?xml") || text.contains("<rss") || text.contains("<feed") {
                text
            } else {
                // Try to detect encoding from content-type header
                if let Some(ref ct_str) = content_type {
                    if let Some(charset) = ct_str
                        .split(';')
                        .find(|part| part.trim().to_lowercase().starts_with("charset="))
                        .and_then(|charset| charset.split('=').nth(1))
                    {
                        result.detected_encoding = Some(charset.trim().to_string());
                        if let Some(encoding) =
                            encoding_rs::Encoding::for_label(charset.trim().as_bytes())
                        {
                            let (decoded, _, _) = encoding.decode(&decompressed_bytes);
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
        Err(_) => {
            // Convert to hex representation for logging
            let hex_preview = decompressed_bytes
                .iter()
                .take(20)
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");

            result.status = RssFeedStatus::InvalidEncoding;
            result.errors.push(format!(
                "Invalid UTF-8 encoding. First 20 bytes: {}",
                hex_preview
            ));

            // Try to detect encoding from content-type header
            if let Some(ref ct_str) = content_type {
                if let Some(charset) = ct_str
                    .split(';')
                    .find(|part| part.trim().to_lowercase().starts_with("charset="))
                    .and_then(|charset| charset.split('=').nth(1))
                {
                    result.detected_encoding = Some(charset.trim().to_string());
                    if let Some(encoding) =
                        encoding_rs::Encoding::for_label(charset.trim().as_bytes())
                    {
                        let (decoded, _, _) = encoding.decode(&decompressed_bytes);
                        decoded.into_owned()
                    } else {
                        result
                            .errors
                            .push(format!("Unsupported encoding: {}", charset.trim()));
                        return Ok(result);
                    }
                } else {
                    // Try Windows-1252 encoding
                    {
                        let (decoded, _, had_errors) =
                            encoding_rs::WINDOWS_1252.decode(&decompressed_bytes);
                        if !had_errors {
                            result.detected_encoding = Some("windows-1252".to_string());
                            result
                                .warnings
                                .push("Auto-detected encoding: windows-1252".to_string());
                            return process_feed_content(
                                decoded.into_owned(),
                                result,
                                content_type,
                                db,
                                url,
                            )
                            .await;
                        }
                    }

                    // Try Shift-JIS encoding
                    {
                        let (decoded, _, had_errors) =
                            encoding_rs::SHIFT_JIS.decode(&decompressed_bytes);
                        if !had_errors {
                            result.detected_encoding = Some("shift_jis".to_string());
                            result
                                .warnings
                                .push("Auto-detected encoding: shift_jis".to_string());
                            return process_feed_content(
                                decoded.into_owned(),
                                result,
                                content_type,
                                db,
                                url,
                            )
                            .await;
                        }
                    }

                    // If all else fails
                    result
                        .errors
                        .push("Could not determine character encoding".to_string());
                    return Ok(result);
                }
            } else {
                result.errors.push(
                    "No content-type with charset specified and content is not valid UTF-8"
                        .to_string(),
                );
                return Ok(result);
            }
        }
    };

    process_feed_content(body, result, content_type, db, url).await
}

// Helper function to process the feed content after extraction
async fn process_feed_content(
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
                let reader = io::Cursor::new(cleaned_xml);
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

// Helper function to add entries to database if a db connection is provided
async fn add_entries_to_database(entries: &[EntryInfo], db: &Database, source_url: &str) {
    for entry in entries {
        if let Some(article_url) = &entry.url {
            // Only attempt to add to queue if we have a valid URL
            match db
                .add_to_queue(
                    article_url,
                    entry.title.as_deref(),
                    entry.pub_date.as_deref(),
                )
                .await
            {
                Ok(true) => {
                    debug!(target: TARGET_WEB_REQUEST, "Test feed: Added article to queue: {}", article_url);
                }
                Ok(false) => {
                    debug!(target: TARGET_WEB_REQUEST, "Test feed: Article already in queue or processed: {}", article_url);
                }
                Err(err) => {
                    error!(target: TARGET_WEB_REQUEST, "Test feed: Failed to add article to queue: {}", err);
                }
            }
        }
    }
    debug!(target: TARGET_WEB_REQUEST, "Test feed: Processed {} articles from {}", entries.len(), source_url);
}

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
                .header(reqwest::header::ACCEPT, "application/feed+json, application/json, application/rss+xml, application/atom+xml, application/xml, text/xml, */*;q=0.9")
                .header(reqwest::header::ACCEPT_ENCODING, "gzip, deflate, br")
                .send()).await
            {
                Ok(Ok(response)) => {
                    debug!(target: TARGET_WEB_REQUEST, "Response Content-Type: {:?}", response.headers().get(reqwest::header::CONTENT_TYPE));

                    let content_type = response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|ct| ct.to_str().ok())
                        .map(|s| s.to_lowercase());

                    if response.status().is_success() {
                        // Get the raw bytes first
                        let bytes = match response.bytes().await {
                            Ok(b) => b,
                            Err(err) => {
                                error!(target: TARGET_WEB_REQUEST, "Failed to read response bytes from {}: {}", rss_url, err);
                                attempts += 1;
                                sleep(RETRY_DELAY).await;
                                continue;
                            }
                        };

                        // Try different decompression methods until one works
                        let decompressed_bytes = {
                            // First try gzip
                            let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
                            let mut decoded = Vec::new();
                            if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                                debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with gzip from {}", rss_url);
                                decoded
                            } else {
                                // Try zlib
                                let mut decoder = flate2::read::ZlibDecoder::new(&bytes[..]);
                                let mut decoded = Vec::new();
                                if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                                    debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with zlib from {}", rss_url);
                                    decoded
                                } else {
                                    // Try deflate
                                    let mut decoder = flate2::read::DeflateDecoder::new(&bytes[..]);
                                    let mut decoded = Vec::new();
                                    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                                        debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with deflate from {}", rss_url);
                                        decoded
                                    } else {
                                        // If no decompression worked, use original bytes
                                        debug!(target: TARGET_WEB_REQUEST, "No decompression method worked for {}, using original bytes", rss_url);
                                        bytes.to_vec()
                                    }
                                }
                            }
                        };

                        // Check if the decompressed data looks like XML
                        if let Ok(text) = String::from_utf8(decompressed_bytes.clone()) {
                            if text.contains("<?xml") || text.contains("<rss") || text.contains("<feed") {
                                debug!(target: TARGET_WEB_REQUEST, "Found XML markers in decompressed data from {}", rss_url);
                            }
                        }

                        // Add debug logging to see what we got after decompression
                        if let Ok(preview) = String::from_utf8(decompressed_bytes[..20.min(decompressed_bytes.len())].to_vec()) {
                            debug!(target: TARGET_WEB_REQUEST, "Decompressed data preview for {}: {}", rss_url, preview);
                        }

                        let body = match String::from_utf8(decompressed_bytes.clone()) {
                            Ok(text) => {
                                if text.starts_with("<?xml") || text.contains("<rss") || text.contains("<feed") {
                                    text
                                } else {
                                    // Try to detect encoding from content-type header
                                    if let Some(ref ct_str) = content_type {
                                        if let Some(charset) = ct_str.split(';')
                                            .find(|part| part.trim().to_lowercase().starts_with("charset="))
                                            .and_then(|charset| charset.split('=').nth(1))
                                        {
                                            if let Some(encoding) = encoding_rs::Encoding::for_label(charset.trim().as_bytes()) {
                                                let (decoded, _, _) = encoding.decode(&decompressed_bytes);
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
                            Err(_) => {
                                // Convert to hex representation for logging if it contains non-printable characters
                                let hex_preview = decompressed_bytes.iter()
                                    .take(20)
                                    .map(|b| format!("{:02x}", b))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                error!(
                                    target: TARGET_WEB_REQUEST,
                                    "Response from {} contains invalid UTF-8. First 20 bytes: {}",
                                    rss_url,
                                    hex_preview
                                );
                                if let Some(ref ct_str) = content_type {
                                    if let Some(charset) = ct_str.split(';')
                                        .find(|part| part.trim().to_lowercase().starts_with("charset="))
                                        .and_then(|charset| charset.split('=').nth(1))
                                    {
                                        if let Some(encoding) = encoding_rs::Encoding::for_label(charset.trim().as_bytes()) {
                                            let (decoded, _, _) = encoding.decode(&decompressed_bytes);
                                            decoded.into_owned()
                                        } else {
                                            attempts += 1;
                                            sleep(RETRY_DELAY).await;
                                            continue;
                                        }
                                    } else {
                                        attempts += 1;
                                        sleep(RETRY_DELAY).await;
                                        continue;
                                    }
                                } else {
                                    attempts += 1;
                                    sleep(RETRY_DELAY).await;
                                    continue;
                                }
                            }
                        };

                        match content_type.as_deref() {
                            Some(ct) if ct.contains("json") => {
                                match serde_json::from_str::<JsonFeed>(&body) {
                                    Ok(feed) => {
                                        for item in feed.items {
                                            if let Some(article_url) = item.url.or(item.id).map(|s| s.to_string()) {
                                                let pub_date = item.date_published.map(|d| {
                                                    if let Some(dt) = parse_date(&d) {
                                                        dt.to_rfc3339()
                                                    } else {
                                                        d.to_string()
                                                    }
                                                });

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
                                                    if let Err(err) = db.add_article(&article_url, false, None, None, None, None, None, None, pub_date.as_deref(), None).await {
                                                        error!(target: TARGET_WEB_REQUEST, "Failed to log old article: {}", err);
                                                    }
                                                    continue;
                                                }

                                                match db.add_to_queue(&article_url, item.title.as_deref(), pub_date.as_deref()).await {
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
                                    Err(err) => {
                                        error!(target: TARGET_WEB_REQUEST, "Failed to parse JSON feed from {}: {}", rss_url, err);
                                        attempts += 1;
                                        sleep(RETRY_DELAY).await;
                                        continue;
                                    }
                                }
                            }
                            _ => {
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
                                                    if let Err(err) = db.add_article(&article_url, false, None, None, None, None, None, None, pub_date.as_deref(), None).await {
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
                                        // Check if it looks like RSS/Atom
                                        if cleaned_xml.contains("<rss") || cleaned_xml.contains("<feed") {
                                            let reader = io::Cursor::new(cleaned_xml);
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
                                                                if let Err(err) = db.add_article(&article_url, false, None, None, None, None, None, None, pub_date.as_deref(), None).await {
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
                                                Err(second_err) => {
                                                    error!(
                                                        target: TARGET_WEB_REQUEST,
                                                        "Failed to parse feed from {} after cleanup. First error: {}. Second error: {}",
                                                        rss_url,
                                                        first_err,
                                                        second_err
                                                    );
                                                    attempts += 1;
                                                    sleep(RETRY_DELAY).await;
                                                    continue;
                                                }
                                            }
                                        } else {
                                            let preview = if body.chars().all(|c| c.is_ascii_graphic() || c.is_whitespace()) {
                                                body.chars().take(100).collect::<String>()
                                            } else {
                                                "[binary data]".to_string()
                                            };
                                            error!(
                                                target: TARGET_WEB_REQUEST,
                                                "Feed from {} doesn't appear to be RSS or Atom. Content preview: {}",
                                                rss_url,
                                                preview
                                            );
                                            attempts += 1;
                                            sleep(RETRY_DELAY).await;
                                            continue;
                                        }
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
