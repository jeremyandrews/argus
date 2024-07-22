use rss::Channel;
use std::io;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;

const TIMEOUT_DURATION: Duration = Duration::from_secs(60);
const RETRY_DELAY: Duration = Duration::from_secs(15);
const MAX_RETRIES: u8 = 3;

pub async fn rss_loop(
    rss_urls: Vec<String>,
    db: Database,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        for rss_url in &rss_urls {
            let mut retries = 0;
            let mut success = false;

            while retries < MAX_RETRIES && !success {
                match timeout(TIMEOUT_DURATION, reqwest::get(rss_url)).await {
                    Ok(Ok(response)) if response.status().is_success() => {
                        debug!("Successfully fetched RSS feed from {}", rss_url);
                        if let Ok(body) = response.text().await {
                            let reader = io::Cursor::new(body);
                            if let Ok(channel) = Channel::read_from(reader) {
                                info!("Parsed RSS feed from {}", rss_url);
                                for item in channel.items() {
                                    if let Some(article_url) = item.link.clone() {
                                        match db.add_to_queue(&article_url).await {
                                            Ok(_) => {
                                                debug!("Added article to queue: {}", article_url)
                                            }
                                            Err(e) => error!(
                                                "Failed to add article to queue: {}, error: {:?}",
                                                article_url, e
                                            ),
                                        }
                                    }
                                }
                            } else {
                                warn!("Failed to parse RSS feed from {}", rss_url);
                            }
                        } else {
                            warn!("Failed to read response body from {}", rss_url);
                        }
                        success = true;
                    }
                    Ok(Ok(response)) => {
                        warn!("Non-success status {} from {}", response.status(), rss_url);
                    }
                    Ok(Err(e)) => {
                        error!("Failed to fetch RSS feed from {}: {:?}", rss_url, e);
                    }
                    Err(e) => {
                        error!("Timeout fetching RSS feed from {}: {:?}", rss_url, e);
                    }
                }

                if !success {
                    retries += 1;
                    warn!(
                        "Retrying {}/{} for RSS feed {}",
                        retries, MAX_RETRIES, rss_url
                    );
                    sleep(RETRY_DELAY).await;
                }
            }

            if !success {
                error!("Exceeded maximum retries for RSS feed {}", rss_url);
            }
        }
        info!("Sleeping for 1 hour before fetching RSS feeds again");
        sleep(Duration::from_secs(3600)).await;
    }
}
