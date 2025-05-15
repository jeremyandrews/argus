//! HTTP client creation and request handling for RSS feeds.

use anyhow::Result;
use reqwest::{cookie::Jar, header};
use std::sync::Arc;
use tokio::time::timeout;
use tracing::{debug, info};

use super::types::{RssFeedStatus, TestRssFeedResult, REQUEST_TIMEOUT};
use crate::TARGET_WEB_REQUEST;

/// Create a client with either standard or browser emulation settings
pub fn create_http_client(browser_emulation: bool) -> Result<reqwest::Client> {
    let cookie_store = Jar::default();
    let builder = reqwest::Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::new(cookie_store))
        .gzip(true)
        .redirect(reqwest::redirect::Policy::default());

    // Add browser-specific settings if needed
    if browser_emulation {
        debug!(target: TARGET_WEB_REQUEST, "Creating browser emulation HTTP client");
    } else {
        debug!(target: TARGET_WEB_REQUEST, "Creating standard HTTP client");
    }

    builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))
}

/// Attempt to fetch a URL with fallback to browser emulation if standard fetch fails
pub async fn fetch_with_fallback(url: &str) -> Result<(reqwest::Response, bool)> {
    // Try standard client first
    debug!(target: TARGET_WEB_REQUEST, "Attempting standard request to {}", url);

    let standard_client = create_http_client(false)?;
    let standard_result = timeout(
        REQUEST_TIMEOUT,
        standard_client
            .get(url)
            .header(header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .header(header::ACCEPT, "application/feed+json, application/json, application/rss+xml, application/atom+xml, application/xml, text/xml, */*;q=0.9")
            .header(header::ACCEPT_ENCODING, "gzip, deflate, br")
            .send(),
    ).await;

    match standard_result {
        Ok(Ok(resp)) if resp.status().is_success() => {
            debug!(target: TARGET_WEB_REQUEST, "Standard request to {} succeeded", url);
            return Ok((resp, false));
        }
        _ => {
            debug!(target: TARGET_WEB_REQUEST, "Standard request to {} failed, trying browser emulation", url);

            // Create error result in case both attempts fail
            let mut error_result = TestRssFeedResult {
                status: RssFeedStatus::RequestFailed,
                content_type: None,
                raw_preview: None,
                decoded_preview: None,
                entries_found: 0,
                detected_encoding: None,
                headers: Vec::new(),
                errors: Vec::new(),
                warnings: Vec::new(),
                entries: Vec::new(),
            };

            match standard_result {
                Ok(Ok(resp)) => {
                    error_result.status = RssFeedStatus::RequestFailed;
                    error_result
                        .errors
                        .push(format!("HTTP error: {}", resp.status()));
                }
                Ok(Err(err)) => {
                    error_result.status = RssFeedStatus::RequestFailed;
                    error_result.errors.push(format!("Request failed: {}", err));
                }
                Err(_) => {
                    error_result.status = RssFeedStatus::RequestTimeout;
                    error_result.errors.push(format!(
                        "Request timed out after {} seconds",
                        REQUEST_TIMEOUT.as_secs()
                    ));
                }
            }

            // Try with browser emulation
            let browser_client = create_http_client(true)?;
            match timeout(
                REQUEST_TIMEOUT,
                browser_client
                    .get(url)
                    .header(header::USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:138.0) Gecko/20100101 Firefox/138.0")
                    .header(header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                    .header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.5")
                    .header(header::ACCEPT_ENCODING, "gzip, deflate, br, zstd")
                    .header("DNT", "1")
                    .header("Upgrade-Insecure-Requests", "1")
                    .header("Connection", "keep-alive")
                    .header("Sec-Fetch-Dest", "document")
                    .header("Sec-Fetch-Mode", "navigate")
                    .header("Sec-Fetch-Site", "none")
                    .header("Sec-Fetch-User", "?1")
                    .header("Priority", "u=0, i")
                    .header("TE", "trailers")
                    .send(),
            ).await {
                Ok(Ok(resp)) if resp.status().is_success() => {
                    info!(target: TARGET_WEB_REQUEST, "Browser emulation request to {} succeeded", url);
                    return Ok((resp, true));
                }
                Ok(Ok(resp)) => {
                    error_result.errors.push(format!("Browser emulation HTTP error: {}", resp.status()));
                    return Err(anyhow::anyhow!("Both standard and browser emulation requests failed"));
                }
                Ok(Err(err)) => {
                    error_result.errors.push(format!("Browser emulation request failed: {}", err));
                    return Err(anyhow::anyhow!("Both standard and browser emulation requests failed"));
                }
                Err(_) => {
                    error_result.errors.push(format!("Browser emulation request timed out after {} seconds", REQUEST_TIMEOUT.as_secs()));
                    return Err(anyhow::anyhow!("Both standard and browser emulation requests failed"));
                }
            }
        }
    }
}
