//! Utility functions for RSS feed processing.

use chrono::{DateTime, Utc};
use flate2;
use std::io::Read;
use tracing::{debug, error};
use url;

use super::types::{EntryInfo, TestRssFeedResult};
use crate::db::core::Database;
use crate::TARGET_WEB_REQUEST;

/// Helper function to validate a URL
pub fn is_valid_url(url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.scheme() == "http" || parsed.scheme() == "https"
    } else {
        false
    }
}

/// Parse a date string in various formats
pub fn parse_date(date_str: &str) -> Option<DateTime<Utc>> {
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

/// Clean up malformed XML
pub fn cleanup_xml(xml: &str) -> String {
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

/// Try various decompression methods for a byte array
pub fn try_decompressions(bytes: &[u8], rss_url: &str) -> Vec<u8> {
    // First try gzip
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with gzip from {}", rss_url);
        return decoded;
    }

    // Try zlib
    let mut decoder = flate2::read::ZlibDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with zlib from {}", rss_url);
        return decoded;
    }

    // Try deflate
    let mut decoder = flate2::read::DeflateDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        debug!(target: TARGET_WEB_REQUEST, "Successfully decompressed with deflate from {}", rss_url);
        return decoded;
    }

    // If no decompression worked, use original bytes
    debug!(target: TARGET_WEB_REQUEST, "No decompression method worked for {}, using original bytes", rss_url);
    bytes.to_vec()
}

/// Try various decompression methods for test interface
pub fn try_other_decompressions(bytes: &[u8], result: &mut TestRssFeedResult) -> Vec<u8> {
    // First try gzip
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        result
            .warnings
            .push("Content was gzip compressed".to_string());
        return decoded;
    }

    // Try zlib
    let mut decoder = flate2::read::ZlibDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        result
            .warnings
            .push("Content was zlib compressed".to_string());
        return decoded;
    }

    // Try deflate
    let mut decoder = flate2::read::DeflateDecoder::new(bytes);
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
        result
            .warnings
            .push("Content was deflate compressed".to_string());
        return decoded;
    }

    // If no decompression worked, use original bytes
    result.warnings.push("No compression detected".to_string());
    bytes.to_vec()
}

/// Helper function to add entries to database if a db connection is provided
pub async fn add_entries_to_database(entries: &[EntryInfo], db: &Database, source_url: &str) {
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
