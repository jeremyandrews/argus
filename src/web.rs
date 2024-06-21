use ollama_rs::Ollama;
use readability::extractor;
use rss::Channel;
use std::io;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::slack::send_to_slack;
use crate::TARGET_WEB_REQUEST;

pub struct ProcessItemParams<'a> {
    pub topics: &'a [String],
    pub ollama: &'a Ollama,
    pub model: &'a str,
    pub temperature: f32,
    pub db: &'a Database,
    pub slack_token: &'a str,
    pub slack_channel: &'a str,
}

pub async fn process_urls(
    urls: Vec<String>,
    params: &ProcessItemParams<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    for url in urls {
        if url.trim().is_empty() {
            continue;
        }

        info!(target: TARGET_WEB_REQUEST, "Loading RSS feed from {}", url);

        match reqwest::get(&url).await {
            Ok(response) => {
                info!(target: TARGET_WEB_REQUEST, "Request to {} succeeded with status {}", url, response.status());
                if response.status().is_success() {
                    let body = response.text().await?;
                    let reader = io::Cursor::new(body);
                    if let Ok(channel) = Channel::read_from(reader) {
                        info!(target: TARGET_WEB_REQUEST, "Parsed RSS channel with {} items", channel.items().len());
                        for item in channel.items() {
                            if let Some(article_url) = item.link.clone() {
                                if params
                                    .db
                                    .has_seen(&article_url)
                                    .expect("Failed to check database")
                                {
                                    info!(target: TARGET_WEB_REQUEST, "Skipping already seen article: {}", article_url);
                                    continue;
                                }
                                process_item(item.clone(), params).await;
                                // Add a 30-second delay after processing each URL
                                debug!(" zzz - sleeping 30 seconds ...");
                                sleep(Duration::from_secs(30)).await;
                            }
                        }
                    } else {
                        error!("Failed to parse RSS channel");
                    }
                } else if response.status() == reqwest::StatusCode::FORBIDDEN {
                    params
                        .db
                        .add_article(&url, false, None, None)
                        .expect("Failed to add URL to database as access denied");
                    warn!(target: TARGET_WEB_REQUEST, "Access denied to {} - added to database to prevent retries", url);
                } else {
                    warn!(target: TARGET_WEB_REQUEST, "Error: Status {} - Headers: {:#?}", response.status(), response.headers());
                }
            }
            Err(err) => {
                error!("Request to {} failed: {}", url, err);
            }
        }
    }
    Ok(())
}

async fn process_item(item: rss::Item, params: &ProcessItemParams<'_>) {
    info!(
        " - reviewing => {} ({})",
        item.title.clone().unwrap_or_default(),
        item.link.clone().unwrap_or_default()
    );
    let article_url = item.link.clone().unwrap_or_default();
    match extract_article_text(&article_url).await {
        Ok(article_text) => {
            for topic in params.topics {
                if topic.trim().is_empty() {
                    continue;
                }

                let prompt = format!("{:?} | {} | \nDetermine whether this is specifically about {}. If it is concisely summarize the information in about 2 paragraphs and then provide a concise one-paragraph analysis of the content and pointing out any logical fallacies if any. Otherwise just reply with the single word 'No', without any further analysis or explanation.", item, article_text, topic);
                if let Some(response_text) = generate_llm_response(&prompt, params).await {
                    if response_text.trim() != "No" {
                        let post_prompt = format!(
                            "Is the article about {}?\n\n{}\n\n{}\n\nRespond with 'Yes' or 'No'.",
                            topic, article_text, response_text
                        );
                        if let Some(post_response) =
                            generate_llm_response(&post_prompt, params).await
                        {
                            if post_response.trim() == "Yes" {
                                let formatted_article = format!(
                                    "*<{}|{}>*",
                                    article_url,
                                    item.title.clone().unwrap_or_default()
                                );
                                send_to_slack(
                                    &formatted_article,
                                    &response_text,
                                    params.slack_token,
                                    params.slack_channel,
                                )
                                .await;
                                params
                                    .db
                                    .add_article(
                                        &article_url,
                                        true,
                                        Some(topic),
                                        Some(&response_text),
                                    )
                                    .expect("Failed to add article to database");
                            } else {
                                info!(
                                    "Article not posted to Slack as per LLM decision: {}",
                                    post_response.trim()
                                );
                            }
                            break;
                        }
                    }
                }
            }
        }
        Err(access_denied) => {
            if access_denied {
                params
                    .db
                    .add_article(&article_url, false, None, None)
                    .expect("Failed to add URL to database as access denied");
                warn!(target: TARGET_WEB_REQUEST, "Access denied for URL: {}", article_url);
            }
        }
    }
}

async fn extract_article_text(url: &str) -> Result<String, bool> {
    let max_retries = 3;
    let article_text: String;
    let mut backoff = 2;

    for retry_count in 0..max_retries {
        let scrape_future = async { extractor::scrape(url) };
        info!(target: TARGET_WEB_REQUEST, "Requesting extraction for URL: {}", url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    warn!(target: TARGET_WEB_REQUEST, "Extracted article is empty for URL: {}", url);
                    break;
                }
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                info!(target: TARGET_WEB_REQUEST, "Extraction succeeded for URL: {}", url);
                return Ok(article_text);
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "Error extracting page: {:?}", e);
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_WEB_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "Failed to extract article after {} retries", max_retries);
                }
                if e.to_string().contains("Access Denied") || e.to_string().contains("Unexpected") {
                    return Err(true);
                }
            }
            Err(_) => {
                warn!(target: TARGET_WEB_REQUEST, "Operation timed out");
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_WEB_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "Failed to extract article after {} retries", max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        }
    }

    warn!(target: TARGET_WEB_REQUEST, "Article text extraction failed for URL: {}", url);
    Err(false)
}
