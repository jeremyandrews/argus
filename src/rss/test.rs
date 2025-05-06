//! RSS feed testing functionality for diagnostics and verification.

use anyhow::Result;
use encoding_rs;
use std::io::Read;

use super::client::fetch_with_fallback;
use super::parser::process_feed_content;
use super::types::{RssFeedStatus, TestRssFeedResult};
use super::util::{is_valid_url, try_other_decompressions};
use crate::db::core::Database;

/// Function to test a single RSS feed with detailed diagnostics
pub async fn test_rss_feed(url: &str, db: Option<&Database>) -> Result<TestRssFeedResult> {
    let mut result = TestRssFeedResult {
        status: RssFeedStatus::Success,
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

    // Validate URL
    if !is_valid_url(url) {
        result.status = RssFeedStatus::RequestFailed;
        result.errors.push(format!("Invalid URL format: {}", url));
        return Ok(result);
    }

    // Use the fetch_with_fallback function to attempt the request
    let (response, browser_emulation_used) = match fetch_with_fallback(url).await {
        Ok((resp, used_emulation)) => (resp, used_emulation),
        Err(err) => {
            result.status = RssFeedStatus::RequestFailed;
            result
                .errors
                .push(format!("All request attempts failed: {}", err));
            return Ok(result);
        }
    };

    // Add a warning if browser emulation was used
    if browser_emulation_used {
        result.warnings.push("Request succeeded using browser emulation (Firefox headers) after standard request failed.".to_string());
    }

    // Extract content type
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .map(|s| s.to_lowercase());

    result.content_type = content_type.clone();

    // Extract HTTP headers
    for (name, value) in response.headers() {
        if let Ok(value_str) = value.to_str() {
            result
                .headers
                .push((name.to_string(), value_str.to_string()));
        }
    }

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
        // Check content-encoding header for compression info
        let content_encoding = result
            .headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "content-encoding")
            .map(|(_, value)| value.to_lowercase());

        // If Brotli compressed (content-encoding: br)
        if content_encoding.as_deref() == Some("br") {
            let mut decoded = Vec::new();
            let mut reader = brotli::Decompressor::new(&bytes[..], 4096);
            if reader.read_to_end(&mut decoded).is_ok() && decoded.len() > 0 {
                result
                    .warnings
                    .push("Content was Brotli compressed".to_string());
                decoded
            } else {
                // If Brotli decompression failed, fall back to other methods
                result
                    .warnings
                    .push("Brotli decompression failed, trying other methods".to_string());
                try_other_decompressions(&bytes, &mut result)
            }
        } else {
            // Try other decompression methods
            try_other_decompressions(&bytes, &mut result)
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
                        .find(|part: &&str| part.trim().to_lowercase().starts_with("charset="))
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
                    .find(|part: &&str| part.trim().to_lowercase().starts_with("charset="))
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
