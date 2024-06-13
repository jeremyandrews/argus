use ollama_rs::generation::options::GenerationOptions;
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use readability::extractor;
use rss::Channel;
use serde_json::json;
use std::{env, io};
use tokio::signal;
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, timeout, Duration};
use tracing::{error, info, warn};
use tracing_appender::rolling;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

const TARGET_WEB_REQUEST: &str = "web_request";
const TARGET_LLM_REQUEST: &str = "llm_request";

mod db;

use db::Database;

struct ProcessItemParams<'a> {
    topics: &'a [String],
    ollama: &'a Ollama,
    model: &'a str,
    temperature: f32,
    cancel_rx: &'a watch::Receiver<bool>,
    db: &'a Database,
    slack_token: &'a str,
    slack_channel: &'a str,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    // Configure the stdout log layer to filter logs at the info level and above, excluding debug logs for `llm_request`
    let stdout_log = fmt::layer()
        .with_writer(io::stdout)
        .with_filter(EnvFilter::new("info,llm_request=off"));

    // Configure the file log layer to filter logs at the debug level for `llm_request` and info level for others
    let file_appender = rolling::daily("logs", "app.log");
    let file_log = fmt::layer()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("web_request=info,llm_request=debug,info"));

    // Initialize the tracing subscriber with both log layers
    Registry::default().with(stdout_log).with(file_log).init();

    // Rest of the main function...

    let (tx, mut rx) = mpsc::channel(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);

    tokio::spawn(async move {
        if signal::ctrl_c().await.is_err() {
            error!("Failed to listen for ctrl-c");
        }
        let _ = cancel_tx.send(true);
        let _ = tx.send(()).await;
    });

    let urls: Vec<String> = env::var("URLS")
        .unwrap_or_default()
        .split(';')
        .map(|url| url.trim().to_string())
        .collect();

    let ollama_host = env::var("OLLAMA_HOST").unwrap_or("localhost".to_string());
    let ollama_port = env::var("OLLAMA_PORT").unwrap_or("11434".to_string());
    let ollama_port: u16 = ollama_port.parse().unwrap_or(11434);

    info!("Connecting to Ollama at {}:{}", ollama_host, ollama_port);

    let ollama = Ollama::new(ollama_host, ollama_port);
    let model = env::var("OLLAMA_MODEL").unwrap_or("llama2".to_string());

    let topics: Vec<String> = env::var("TOPICS")
        .unwrap_or_default()
        .split(';')
        .map(|topic| topic.trim().to_string())
        .collect();

    let slack_token = env::var("SLACK_TOKEN").expect("SLACK_TOKEN environment variable required");
    let slack_channel =
        env::var("SLACK_CHANNEL").expect("SLACK_CHANNEL environment variable required");

    let db_path = env::var("DATABASE_PATH").unwrap_or("argus.db".to_string());
    let db = Database::new(&db_path).expect("Failed to initialize database");

    let temperature: f32 = env::var("LLM_TEMPERATURE")
        .unwrap_or("0.0".to_string())
        .parse()
        .unwrap_or(0.0);

    let params = ProcessItemParams {
        topics: &topics,
        ollama: &ollama,
        model: &model,
        temperature,
        cancel_rx: &cancel_rx,
        db: &db,
        slack_token: &slack_token,
        slack_channel: &slack_channel,
    };

    for url in urls {
        if url.trim().is_empty() {
            continue;
        }

        info!("Loading RSS feed from {}", url);

        let res = reqwest::get(&url).await;
        match res {
            Ok(response) => {
                info!(target: TARGET_WEB_REQUEST, "Request to {} succeeded with status {}", url, response.status());
                if !response.status().is_success() {
                    warn!(
                        target: TARGET_WEB_REQUEST,
                        "Error: Status {} - Headers: {:#?}",
                        response.status(),
                        response.headers()
                    );
                    continue;
                }

                let body = response.text().await?;
                let reader = io::Cursor::new(body);
                let channel = Channel::read_from(reader).unwrap();

                info!("Parsed RSS channel with {} items", channel.items().len());

                let items: Vec<rss::Item> = channel.items().to_vec();

                for item in items {
                    let article_url = item.link.clone().unwrap_or_default();
                    if db.has_seen(&article_url).expect("Failed to check database") {
                        info!(" o Skipping already seen article: {}", article_url);
                        continue;
                    }

                    tokio::select! {
                        _ = rx.recv() => {
                            info!("Ctrl-C received, stopping article processing.");
                            return Ok(());
                        },
                        _ = process_item(item, &params) => {}
                    }
                }
            }
            Err(err) => {
                error!("Request to {} failed: {:?}", url, err);
            }
        }
    }

    Ok(())
}

async fn process_item<'a>(item: rss::Item, params: &ProcessItemParams<'a>) {
    info!(" - reviewing => {}", item.title.clone().unwrap_or_default());

    let article_url = item.link.clone().unwrap_or_default();
    let mut article_text = String::new();
    let max_retries = 3;

    for retry_count in 0..max_retries {
        if *params.cancel_rx.borrow() {
            info!("Cancellation received, stopping retries.");
            return;
        }

        let scrape_future = async { extractor::scrape(&article_url) };
        info!(target: TARGET_WEB_REQUEST, "Requesting extraction for URL: {}", article_url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                info!(target: TARGET_WEB_REQUEST, "Extraction succeeded for URL: {}", article_url);
                break;
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "Error extracting page: {}", e);
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

    if article_text.is_empty() {
        return;
    }

    for topic in params.topics {
        if topic.trim().is_empty() {
            continue;
        }

        let prompt: String = format!("{:?} | {} | \nDetermine whether this is specifically about {}. If it is concisely summarize the information in about 2 paragraphs and then provide a concise one-paragraph analysis of the content and pointing out any logical fallacies if any. Otherwise just reply with the single word 'No', without any further analysis or explanation.", item, article_text, topic);

        let mut response_text = String::new();

        for retry_count in 0..max_retries {
            if *params.cancel_rx.borrow() {
                info!("Cancellation received, stopping retries.");
                return;
            }

            let mut request = GenerationRequest::new(params.model.to_string(), prompt.clone());
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

        if response_text.trim() != "No" {
            let post_prompt: String = format!(
                "Is the article about {}?\n\n{}\n\n{}\n\nRespond with 'Yes' or 'No'.",
                topic, article_text, response_text
            );

            let mut post_response = String::new();

            for retry_count in 0..max_retries {
                if *params.cancel_rx.borrow() {
                    info!("Cancellation received, stopping retries.");
                    return;
                }

                let mut post_request =
                    GenerationRequest::new(params.model.to_string(), post_prompt.clone());
                post_request.options =
                    Some(GenerationOptions::default().temperature(params.temperature));

                info!(target: TARGET_LLM_REQUEST, "Sending LLM post request with prompt: {}", post_prompt);
                match timeout(
                    Duration::from_secs(60),
                    params.ollama.generate(post_request),
                )
                .await
                {
                    Ok(Ok(response)) => {
                        post_response = response.response;
                        info!(target: TARGET_LLM_REQUEST, "LLM post response: {}", post_response);
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!(target: TARGET_LLM_REQUEST, "Error generating post response: {}", e);
                        if retry_count < max_retries - 1 {
                            info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                        } else {
                            error!(
                                target: TARGET_LLM_REQUEST,
                                "Failed to generate post response after {} retries",
                                max_retries
                            );
                        }
                    }
                    Err(_) => {
                        warn!(target: TARGET_LLM_REQUEST, "Operation timed out");
                        if retry_count < max_retries - 1 {
                            info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                        } else {
                            error!(
                                target: TARGET_LLM_REQUEST,
                                "Failed to generate post response after {} retries",
                                max_retries
                            );
                        }
                    }
                }

                if retry_count < max_retries - 1 {
                    sleep(Duration::from_secs(2)).await;
                }
            }

            if post_response.trim() == "Yes" {
                let formatted_article = format!(
                    "*<{}|{}>*",
                    article_url,
                    item.title.clone().unwrap_or_default()
                );
                let formatted_response = response_text.clone();

                info!(" ++ matched {}.", topic);

                send_to_slack(
                    &formatted_article,
                    &formatted_response,
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

    params
        .db
        .add_article(&article_url, false, None, None)
        .expect("Failed to add article to database");
}

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
