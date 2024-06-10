use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};
use readability::extractor;
use rss::Channel;
use serde_json::json;
use std::collections::HashMap;
use std::{env, io};
use tokio::signal;
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let (tx, mut rx) = mpsc::channel(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);

    tokio::spawn(async move {
        if signal::ctrl_c().await.is_err() {
            eprintln!("Failed to listen for ctrl-c");
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

    println!("Connecting to Ollama at {}:{}", ollama_host, ollama_port);

    let ollama = Ollama::new(ollama_host, ollama_port);
    let model = env::var("OLLAMA_MODEL").unwrap_or("llama2".to_string());

    let topics: Vec<String> = env::var("TOPICS")
        .unwrap_or_default()
        .split(';')
        .map(|topic| topic.trim().to_string())
        .collect();

    let slack_webhook_url =
        env::var("SLACK_WEBHOOK_URL").expect("SLACK_WEBHOOK_URL environment variable required");

    let mut topic_articles: HashMap<&str, Vec<String>> = HashMap::new();

    for url in urls {
        if url.trim().is_empty() {
            continue;
        }

        println!("Loading RSS feed from {}", url);

        let res = reqwest::get(&url).await?;
        if !res.status().is_success() {
            println!(
                "Error: Status {} - Headers: {:#?}",
                res.status(),
                res.headers()
            );
            continue;
        }

        let body = res.text().await?;
        let reader = io::Cursor::new(body);
        let channel = Channel::read_from(reader).unwrap();

        println!("Parsed RSS channel with {} items", channel.items().len());

        let items: Vec<rss::Item> = channel.items().to_vec();

        for item in items {
            tokio::select! {
                _ = rx.recv() => {
                    println!("Ctrl-C received, stopping article processing.");
                    send_summary(&topic_articles, &slack_webhook_url).await;
                    return Ok(());
                },
                _ = process_item(item, &topics, &ollama, &model, &mut topic_articles, &cancel_rx) => {}
            }
        }
    }

    send_summary(&topic_articles, &slack_webhook_url).await;

    Ok(())
}

async fn process_item<'a>(
    item: rss::Item,
    topics: &'a [String],
    ollama: &'a Ollama,
    model: &'a str,
    topic_articles: &mut HashMap<&'a str, Vec<String>>,
    cancel_rx: &watch::Receiver<bool>,
) {
    println!(" - reviewing => {}", item.title.clone().unwrap_or_default());

    let article_url = item.link.clone().unwrap_or_default();
    let mut article_text = String::new();
    let max_retries = 3;

    for retry_count in 0..max_retries {
        if *cancel_rx.borrow() {
            println!("Cancellation received, stopping retries.");
            return;
        }

        let scrape_future = async { extractor::scrape(&article_url) };
        match timeout(Duration::from_secs(5), scrape_future).await {
            Ok(Ok(product)) => {
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                break;
            }
            Ok(Err(e)) => {
                println!("Error extracting page: {}", e);
                if retry_count < max_retries - 1 {
                    println!("Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    println!("Failed to extract article after {} retries", max_retries);
                }
            }
            Err(_) => {
                println!("Operation timed out");
                if retry_count < max_retries - 1 {
                    println!("Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    println!("Failed to extract article after {} retries", max_retries);
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

    for topic in topics {
        if topic.trim().is_empty() {
            continue;
        }

        let prompt: String = format!("{:?} | {} | \nDetermine whether this is specifically about {}. If it is concisely summarize the information in about 2 paragraphs and then provide a concise one-paragraph analysis of the content and pointing out any logical fallacies if any. Otherwise just reply 'No', without any further analysis or explanation.", item, article_text, topic);

        let response = ollama
            .generate(GenerationRequest::new(model.to_string(), prompt))
            .await;

        let response_text = response.map(|r| r.response).unwrap_or_else(|err| {
            eprintln!("Error generating response: {}", err);
            "Error generating response".to_string()
        });

        if response_text.trim() != "No" {
            let formatted_article = format!(
                "*<{}|{}>*",
                article_url,
                item.title.clone().unwrap_or_default()
            );
            let formatted_response = response_text;

            topic_articles.entry(topic).or_default().push(
                json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": formatted_article
                    }
                })
                .to_string(),
            );
            topic_articles.entry(topic).or_default().push(
                json!({
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": formatted_response
                    }
                })
                .to_string(),
            );

            println!(" ++ matched {}.", topic);
            break; // log to the first matching topic and break
        }
    }
}

async fn send_summary(topic_articles: &HashMap<&str, Vec<String>>, slack_webhook_url: &str) {
    let mut blocks = vec![];

    for (topic, articles) in topic_articles {
        let header_block = json!({
            "type": "header",
            "text": json!({
                "type": "plain_text",
                "text": topic,
                "emoji": true,
            }),
        });
        blocks.push(header_block);

        for article in articles {
            let article_block: serde_json::Value = match serde_json::from_str(article) {
                Ok(block) => block,
                Err(e) => {
                    eprintln!("Error parsing block: {}", e);
                    continue;
                }
            };
            blocks.push(article_block);
        }

        let divider_block = json!({
            "type": "divider",
        });
        blocks.push(divider_block);
    }

    if blocks.is_empty() {
        println!("No articles matched, nothing to send to Slack.");
        return;
    }

    let client = reqwest::Client::new();
    let payload = json!({
        "blocks": blocks,
    });

    let res = client
        .post(slack_webhook_url)
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                println!("Slack notification sent successfully");
            } else {
                let error_text = response.text().await.unwrap_or_default();
                eprintln!("Error sending Slack notification: {}", error_text);
                eprintln!("Payload: {}", payload);
            }
        }
        Err(err) => {
            eprintln!("Error sending Slack notification: {:?}", err);
        }
    }
}
