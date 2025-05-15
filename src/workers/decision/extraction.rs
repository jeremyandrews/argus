use crate::{WorkerDetail, TARGET_WEB_REQUEST};
use readability::extractor;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, warn};

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
pub async fn extract_article_text(
    url: &str,
    worker_detail: &WorkerDetail,
) -> Result<(String, String), bool> {
    let max_retries = 3;
    let article_text: String;
    let article_html: String;
    let mut backoff = 2;

    for retry_count in 0..max_retries {
        let scrape_future = async { extractor::scrape(url) };
        debug!(target: TARGET_WEB_REQUEST, "[{} {} {}]: extracting URL: {}.", worker_detail.name, worker_detail.id, worker_detail.model, url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    // @TODO: handle this another way
                    warn!(target: TARGET_WEB_REQUEST, "[{} {} {}]: extracted empty article from URL: {}.", worker_detail.name, worker_detail.id, worker_detail.model, url);
                    break;
                }
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                article_html = product.content.clone();

                debug!(target: TARGET_WEB_REQUEST, "[{} {} {}]: successfully extracted URL: {}.", worker_detail.name, worker_detail.id, worker_detail.model, url);
                return Ok((article_text, article_html));
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "[{} {} {}]: error extracting URL: {} ({:#?}).", worker_detail.name, worker_detail.id, worker_detail.model, url, e);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "[{} {} {}]: retrying URL: {} ({}/{}).", worker_detail.name, worker_detail.id, worker_detail.model, url, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "[{} {} {}]: failed to load URL: {} after {} tries.", worker_detail.name, worker_detail.id, worker_detail.model, url, max_retries);
                }
                if e.to_string().contains("Access Denied") || e.to_string().contains("Unexpected") {
                    return Err(true);
                }
            }
            Err(_) => {
                warn!(target: TARGET_WEB_REQUEST, "[{} {} {}]: operation timed out.", worker_detail.name, worker_detail.id, worker_detail.model);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "[{} {} {}]: retrying URL: {} ({}/{}).", worker_detail.name, worker_detail.id, worker_detail.model, url, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "[{} {} {}]: failed to load URL: {} after {} tries.", worker_detail.name, worker_detail.id, worker_detail.model, url, max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }

    warn!(target: TARGET_WEB_REQUEST, "[{} {} {}]: failed to extract URL: {}.", worker_detail.name, worker_detail.id, worker_detail.model, url);
    Err(false)
}

/// Handles the case where access to the article is denied, updating the database and logging a warning.
pub async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    article_title: &str,
    title_domain_hash: &str,
    pub_date: Option<&str>,
    db: &crate::db::core::Database,
    worker_detail: &WorkerDetail,
) {
    if access_denied {
        match db
            .add_article(
                article_url,
                false,
                None,
                None,
                None,
                None,
                Some(title_domain_hash),
                None,
                pub_date,
                None, // event_date
            )
            .await
        {
            Ok(_) => {
                warn!(target: TARGET_WEB_REQUEST, "[{} {} {}]: access denied for URL: {} ({}).", worker_detail.name, worker_detail.id, worker_detail.model, article_url, article_title);
            }
            Err(e) => {
                error!(target: TARGET_WEB_REQUEST, "[{} {} {}]: failed to add access denied URL {} ({}) to database: {:?}.", worker_detail.name, worker_detail.id, worker_detail.model, article_url, article_title, e);
            }
        }
    }
}
