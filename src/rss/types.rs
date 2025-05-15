//! Type definitions for the RSS module.

use serde::{Deserialize, Serialize};
use tokio::time::Duration;

/// Diagnostic status codes for RSS feed testing
#[derive(Debug, Clone, Serialize)]
pub enum RssFeedStatus {
    Success,
    InvalidEncoding,
    NotRssOrAtom,
    RequestFailed,
    ParseError,
    RequestTimeout,
}

/// Detailed test results for an RSS feed
#[derive(Debug, Clone, Serialize)]
pub struct TestRssFeedResult {
    pub status: RssFeedStatus,
    pub content_type: Option<String>,
    pub raw_preview: Option<Vec<u8>>,
    pub decoded_preview: Option<String>,
    pub entries_found: usize,
    pub detected_encoding: Option<String>,
    pub headers: Vec<(String, String)>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub entries: Vec<EntryInfo>,
}

/// Basic information about a feed entry
#[derive(Debug, Clone, Serialize)]
pub struct EntryInfo {
    pub title: Option<String>,
    pub url: Option<String>,
    pub pub_date: Option<String>,
}

/// JSON feed structure for parsing
#[derive(Debug, Deserialize)]
pub struct JsonFeed {
    #[serde(default)]
    pub items: Vec<JsonFeedItem>,
}

/// JSON feed item structure
#[derive(Debug, Deserialize)]
pub struct JsonFeedItem {
    pub id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub date_published: Option<String>,
}

// Constants
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const RETRY_DELAY: Duration = Duration::from_secs(5);
pub const MAX_RETRIES: usize = 3;
