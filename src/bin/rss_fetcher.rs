use anyhow::Result;
use feed_rs::parser;
use rand::seq::SliceRandom;
use rand::Rng;
use readability::extractor;
use serde_json::json;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use tracing_subscriber;

use argus::llm;
use argus::prompts;
use argus::{LLMClient, LLMParams};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TEST_DATA_DIR: &str = "test_data";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    info!("Starting RSS fetcher...");

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: rss_fetcher <RSS_URL>");
        std::process::exit(1);
    }
    let rss_url = &args[1];

    debug!(target: "config", "Environment Variables - LLM_BASE_URL: {}, LLM_MODEL: {}",
        std::env::var("LLM_BASE_URL").unwrap_or_default(),
        std::env::var("LLM_MODEL").unwrap_or_default()
    );

    let feed = fetch_rss_feed(rss_url).await?;
    let mut rng = rand::thread_rng();
    let articles: Vec<_> = feed.entries.choose_multiple(&mut rng, 5).collect();

    let output_dir = Path::new(TEST_DATA_DIR);
    create_dir_all(output_dir)?;

    for entry in articles {
        debug!(target: "article", "Selected article metadata - Title: {:?}, Link: {}",
            entry.title.clone(),
            entry.links.first().map(|l| l.href.clone()).unwrap_or_default()
        );

        if let Some(link) = entry.links.first() {
            info!(target: "article", "Processing article from link: {}", link.href);
            let source = extract_source(rss_url);
            let random_blob: String = (0..8)
                .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
                .collect();
            let filename = format!(
                "{}-{}--{}.json",
                source,
                chrono::Local::now().format("%m%d"),
                random_blob
            );
            let file_path = output_dir.join(filename);
            debug!(target: "file", "Generated filename: {}", file_path.display());

            match extract_article_text(&link.href).await {
                Ok(article) => {
                    info!(target: "article", "Extracted article titled: {}", article.title);
                    let mut relevance = json!({});
                    for (topic_key, topic_name) in get_topics() {
                        let prompt = crate::prompts::is_this_about(&article.body, topic_name);
                        debug!(target: "prompt", "Generated prompt for topic '{}': {}", topic_name, prompt);

                        // Get environment variables
                        let llm_base_url = std::env::var("LLM_BASE_URL")
                            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
                        info!(target: "config", "Using LLM base URL: {}", llm_base_url);
                        let url =
                            url::Url::parse(&llm_base_url).expect("Invalid LLM_BASE_URL format");
                        let llm_host_with_scheme = format!(
                            "{}://{}",
                            url.scheme(),
                            url.host_str().expect("Missing host in LLM_BASE_URL")
                        );
                        let llm_port = url.port().unwrap_or(11434); // Default to 11434 if no port is specified
                        info!(target: "config", "LLM host with scheme: {}, port: {}", llm_host_with_scheme, llm_port);
                        let llm_model =
                            std::env::var("LLM_MODEL").unwrap_or_else(|_| "llama3.1".to_string());
                        info!(target: "config", "Using LLM model: {}", llm_model);

                        let llm_params = crate::LLMParams {
                            llm_client: &LLMClient::Ollama(ollama_rs::Ollama::new(
                                llm_host_with_scheme.clone(),
                                llm_port,
                            )),
                            model: &llm_model, // Borrow the model as a &str
                            temperature: 0.0,
                        };

                        if let Some(response) =
                            crate::llm::generate_llm_response(&prompt, &llm_params).await
                        {
                            relevance[topic_key] =
                                serde_json::Value::String(response.trim().to_string());
                            info!(target: "llm", "Received response for topic '{}': {}", topic_key, response.trim());
                        } else {
                            relevance[topic_key] = serde_json::Value::String("unknown".to_string());
                            warn!(target: "llm", "No response for topic '{}'", topic_key);
                        }
                    }
                    save_article_to_json_file(
                        &file_path,
                        &article.title,
                        &article.body,
                        &relevance,
                    )
                    .unwrap_or_else(|e| {
                        error!(target: "file", "Failed to save article to {}: {}", file_path.display(), e);
                        panic!("Failed to save article");
                    });
                    info!(target: "file", "Saved article to {}", file_path.display());
                }
                Err(e) => {
                    error!(target: "article", "Failed to extract article from {}: {}", link.href, e);
                }
            }
        } else {
            warn!(target: "article", "No valid link found in feed entry.");
        }
    }

    Ok(())
}

async fn fetch_rss_feed(rss_url: &str) -> Result<feed_rs::model::Feed> {
    info!(target: "feed", "Fetching RSS feed from URL: {}", rss_url);
    let response = timeout(REQUEST_TIMEOUT, reqwest::get(rss_url)).await??;
    let body = response.text().await?;
    debug!(target: "feed", "Received RSS feed content of length: {}", body.len());

    let reader = std::io::Cursor::new(body);
    let feed = parser::parse(reader)?;
    info!(target: "feed", "Parsed RSS feed successfully with {} entries.", feed.entries.len());
    debug!(target: "feed", "First entry title: {:?}", feed.entries.first().and_then(|e| e.title.clone()));

    Ok(feed)
}

fn extract_source(url: &str) -> String {
    if let Ok(parsed_url) = url::Url::parse(url) {
        parsed_url.host_str().unwrap_or("unknown").to_string()
    } else {
        "unknown".to_string()
    }
}

async fn extract_article_text(url: &str) -> Result<Article> {
    const MAX_RETRIES: usize = 3;
    let mut backoff = 2;

    for retry_count in 0..MAX_RETRIES {
        let scrape_future = async { extractor::scrape(url) };
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    return Err(anyhow::anyhow!("Extracted article is empty"));
                }
                let article = Article {
                    title: product.title,
                    body: product.text,
                };
                return Ok(article);
            }
            Ok(Err(e)) => {
                eprintln!("Error extracting page: {:?}", e);
                if retry_count < MAX_RETRIES - 1 {
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff *= 2;
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to extract article after {} retries",
                        MAX_RETRIES
                    ));
                }
            }
            Err(_) => {
                eprintln!("Operation timed out");
                if retry_count < MAX_RETRIES - 1 {
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                    backoff *= 2;
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to extract article after {} retries",
                        MAX_RETRIES
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Article text extraction failed for URL: {}",
        url
    ))
}

fn save_article_to_json_file(
    file_path: &Path,
    title: &str,
    body: &str,
    relevance: &serde_json::Value,
) -> Result<()> {
    let article_json = json!({
        "title": title,
        "body": body,
        "relevance": relevance,
    });

    let mut file = File::create(file_path)?;
    file.write_all(article_json.to_string().as_bytes())?;
    Ok(())
}

#[derive(Debug)]
struct Article {
    title: String,
    body: String,
}

fn get_topics() -> Vec<(&'static str, &'static str)> {
    vec![
        ("apple", "New Apple products, like new versions of iPhone, iPad and MacBooks, or newly announced products"),
        ("space", "Space and Space Exploration"),
        ("longevity", "Advancements in health practices and technologies that enhance human longevity"),
        ("llm", "significant new developments in Large Language Models, or anything about the Llama LLM"),
        ("ev", "Electric vehicles"),
        ("rust", "the Rust programming language"),
        ("bitcoin", "Bitcoins, the cryptocurrency"),
        ("drupal", "the Drupal Content Management System"),
        ("linux_vuln", "a major new vulnerability in Linux, macOS, or iOS"),
        ("global_vuln", "a global vulnerability bringing down significant infrastructure worldwide"),
        ("tuscany", "Tuscany, the famous region in Italy"),
    ]
}
