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

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TEST_DATA_DIR: &str = "test_data";

#[tokio::main]
async fn main() -> Result<()> {
    // Get the RSS URL from the command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: rss_fetcher <RSS_URL>");
        std::process::exit(1);
    }
    let rss_url = &args[1];

    // Fetch and parse the RSS feed
    let feed = fetch_rss_feed(rss_url).await?;

    // Select up to 2 random articles
    let mut rng = rand::thread_rng();
    let articles: Vec<_> = feed.entries.choose_multiple(&mut rng, 2).collect();

    // Ensure the test data directory exists
    let output_dir = Path::new(TEST_DATA_DIR);
    create_dir_all(output_dir)?;

    // Save each article to a file
    for entry in articles {
        if let Some(link) = entry.links.first() {
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

            match extract_article_text(&link.href).await {
                Ok(article) => {
                    save_article_to_json_file(&file_path, &article.title, &article.body)
                        .expect("Failed to save article");
                    println!("Saved article to {}", file_path.display());
                }
                Err(_) => {
                    eprintln!("Failed to extract article from {}", link.href);
                }
            }
        }
    }

    Ok(())
}

async fn fetch_rss_feed(rss_url: &str) -> Result<feed_rs::model::Feed> {
    let response = timeout(REQUEST_TIMEOUT, reqwest::get(rss_url)).await??;
    let body = response.text().await?;
    let reader = std::io::Cursor::new(body);
    let feed = parser::parse(reader)?;
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

fn save_article_to_json_file(file_path: &Path, title: &str, body: &str) -> Result<()> {
    let article_json = json!({
        "title": title,
        "body": body,
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
