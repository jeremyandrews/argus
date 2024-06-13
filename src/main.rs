use ollama_rs::generation::options::GenerationOptions;
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use readability::extractor;
use rss::Channel;
use serde_json::json;
use std::{env, io};
use tokio::time::{sleep, timeout, Duration};
use tracing::{error, info, warn};
use tracing_appender::rolling;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

const TARGET_WEB_REQUEST: &str = "web_request";
const TARGET_LLM_REQUEST: &str = "llm_request";

mod db;

use db::Database;

/// Struct to hold parameters for processing items.
struct ProcessItemParams<'a> {
    topics: &'a [String],
    ollama: &'a Ollama,
    model: &'a str,
    temperature: f32,
    db: &'a Database,
    slack_token: &'a str,
    slack_channel: &'a str,
}

/// Main function to configure logging and process URLs asynchronously.
///
/// # Returns
/// - `Result<(), Box<dyn std::error::Error>>`
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_logging();

    let urls = get_env_var_as_vec("URLS", ';');
    let ollama_host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "localhost".to_string());
    let ollama_port = env::var("OLLAMA_PORT")
        .unwrap_or_else(|_| "11434".to_string())
        .parse()
        .unwrap_or(11434);

    info!(target: TARGET_LLM_REQUEST, "Connecting to Ollama at {}:{}", ollama_host, ollama_port);
    let ollama = Ollama::new(ollama_host, ollama_port);
    let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());
    let topics = get_env_var_as_vec("TOPICS", ';');
    let slack_token = env::var("SLACK_TOKEN").expect("SLACK_TOKEN environment variable required");
    let slack_channel =
        env::var("SLACK_CHANNEL").expect("SLACK_CHANNEL environment variable required");
    let db_path = env::var("DATABASE_PATH").unwrap_or_else(|_| "argus.db".to_string());
    let db = Database::new(&db_path).expect("Failed to initialize database");
    let temperature = env::var("LLM_TEMPERATURE")
        .unwrap_or_else(|_| "0.0".to_string())
        .parse()
        .unwrap_or(0.0);

    let params = ProcessItemParams {
        topics: &topics,
        ollama: &ollama,
        model: &model,
        temperature,
        db: &db,
        slack_token: &slack_token,
        slack_channel: &slack_channel,
    };

    process_urls(urls, &params).await?;

    Ok(())
}

/// Configures the logging system to log to stdout and a file.
fn configure_logging() {
    let stdout_log = fmt::layer()
        .with_writer(io::stdout)
        .with_filter(EnvFilter::new("info,llm_request=warn,web_request=warn"));

    let file_appender = rolling::daily("logs", "app.log");
    let file_log = fmt::layer()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("web_request=info,llm_request=debug,info"));

    Registry::default().with(stdout_log).with(file_log).init();
}

/// Retrieves an environment variable and splits it into a vector of strings based on a delimiter.
///
/// # Arguments
/// - `var`: The name of the environment variable.
/// - `delimiter`: The character to split the environment variable's value by.
///
/// # Returns
/// - `Vec<String>`
fn get_env_var_as_vec(var: &str, delimiter: char) -> Vec<String> {
    env::var(var)
        .unwrap_or_default()
        .split(delimiter)
        .map(|s| s.trim().to_string())
        .collect()
}

/// Processes a list of URLs by loading RSS feeds and handling each item.
///
/// # Arguments
/// - `urls`: A vector of URLs to process.
/// - `params`: Parameters for processing items.
///
/// # Returns
/// - `Result<(), Box<dyn std::error::Error>>`
async fn process_urls(
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

/// Processes a single RSS item by extracting the article text, generating a response, and potentially sending it to Slack.
///
/// # Arguments
/// - `item`: The RSS item to process.
/// - `params`: Parameters for processing items.
async fn process_item(item: rss::Item, params: &ProcessItemParams<'_>) {
    info!(
        " - reviewing => {} ({})",
        item.title.clone().unwrap_or_default(),
        item.link.clone().unwrap_or_default()
    );
    let article_url = item.link.clone().unwrap_or_default();
    let article_text = extract_article_text(&article_url).await;
    if article_text.is_none() {
        return;
    }

    for topic in params.topics {
        if topic.trim().is_empty() {
            continue;
        }

        let prompt = format!("{:?} | {} | \nDetermine whether this is specifically about {}. If it is concisely summarize the information in about 2 paragraphs and then provide a concise one-paragraph analysis of the content and pointing out any logical fallacies if any. Otherwise just reply with the single word 'No', without any further analysis or explanation.", item, article_text.as_ref().unwrap(), topic);
        if let Some(response_text) = generate_llm_response(&prompt, params).await {
            if response_text.trim() != "No" {
                let post_prompt = format!(
                    "Is the article about {}?\n\n{}\n\n{}\n\nRespond with 'Yes' or 'No'.",
                    topic,
                    article_text.as_ref().unwrap(),
                    response_text
                );
                if let Some(post_response) = generate_llm_response(&post_prompt, params).await {
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
                            .add_article(&article_url, true, Some(topic), Some(&response_text))
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

    params
        .db
        .add_article(&article_url, false, None, None)
        .expect("Failed to add article to database");
}

/// Extracts the text of an article from a given URL with retries.
///
/// # Arguments
/// - `url`: The URL of the article to extract.
///
/// # Returns
/// - `Option<String>`: The extracted article text, or `None` if extraction failed.
async fn extract_article_text(url: &str) -> Option<String> {
    let max_retries = 3;
    let mut article_text = None;

    for retry_count in 0..max_retries {
        let scrape_future = async { extractor::scrape(url) };
        info!(target: TARGET_WEB_REQUEST, "Requesting extraction for URL: {}", url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    warn!(target: TARGET_WEB_REQUEST, "Extracted article is empty for URL: {}", url);
                    break;
                }
                article_text = Some(format!(
                    "Title: {}\nBody: {}\n",
                    product.title, product.text
                ));
                info!(target: TARGET_WEB_REQUEST, "Extraction succeeded for URL: {}", url);
                break;
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "Error extracting page: {:?}", e);
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_WEB_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "Failed to extract article after {} retries", max_retries);
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
            sleep(Duration::from_secs(2)).await;
        }
    }

    if article_text.is_none() {
        warn!(target: TARGET_WEB_REQUEST, "Article text extraction failed for URL: {}", url);
    }

    article_text
}

/// Generates a response from the LLM based on a given prompt with retries.
///
/// # Arguments
/// - `prompt`: The prompt to send to the LLM.
/// - `params`: Parameters for processing items.
///
/// # Returns
/// - `Option<String>`
async fn generate_llm_response(prompt: &str, params: &ProcessItemParams<'_>) -> Option<String> {
    let max_retries = 3;
    let mut response_text = String::new();

    for retry_count in 0..max_retries {
        let mut request = GenerationRequest::new(params.model.to_string(), prompt.to_string());
        request.options = Some(GenerationOptions::default().temperature(params.temperature));

        info!(target: TARGET_LLM_REQUEST, "Sending LLM request with prompt: {}", prompt);
        match timeout(Duration::from_secs(60), params.ollama.generate(request)).await {
            Ok(Ok(response)) => {
                response_text = response.response;
                info!(target: TARGET_LLM_REQUEST, "LLM response: {}", response_text);
                break;
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_LLM_REQUEST, "Error generating response: {}", e);
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_LLM_REQUEST, "Failed to generate response after {} retries", max_retries);
                }
            }
            Err(_) => {
                warn!(target: TARGET_LLM_REQUEST, "Operation timed out");
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_LLM_REQUEST, "Failed to generate response after {} retries", max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(2)).await;
        }
    }

    if response_text.is_empty() {
        None
    } else {
        Some(response_text)
    }
}

/// Sends a message to a Slack channel with the article and response text.
///
/// # Arguments
/// - `article`: The article text to send.
/// - `response`: The LLM response text to send.
/// - `slack_token`: The Slack API token.
/// - `slack_channel`: The Slack channel ID.
async fn send_to_slack(article: &str, response: &str, slack_token: &str, slack_channel: &str) {
    let client = reqwest::Client::new();
    let payload = json!({
        "channel": slack_channel,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": article
                }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": response
                }
            }
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });

    info!(target: TARGET_WEB_REQUEST, "Sending Slack notification with payload: {}", payload);
    let res = client
        .post("https://slack.com/api/chat.postMessage")
        .header("Content-Type", "application/json")
        .bearer_auth(slack_token)
        .body(payload.to_string())
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                info!(" ** Slack notification sent successfully");
            } else {
                let error_text = response.text().await.unwrap_or_default();
                error!(" !! Error sending Slack notification: {}", error_text);
                error!(" !! Payload: {}", payload);
            }
        }
        Err(err) => {
            error!(" !! Error sending Slack notification: {:?}", err);
        }
    }
}
