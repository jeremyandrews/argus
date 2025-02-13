use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use rand::{rngs::StdRng, Rng, SeedableRng};
use readability::extractor;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::db::Database;
use crate::llm::{generate_llm_response, ThreatLocationResponse};
use crate::prompts;
use crate::util::{parse_places_data_hierarchical, weighted_sleep};
use crate::{LLMClient, LLMParams, WorkerDetail};
use crate::{TARGET_DB, TARGET_LLM_REQUEST, TARGET_WEB_REQUEST};

/// Parameters required for processing an item, including topics, database, and Slack channel information.
pub struct ProcessItemParams<'a> {
    pub topics: &'a [String],
    pub llm_client: &'a LLMClient,
    pub model: &'a str,
    pub temperature: f32,
    pub db: &'a Database,
    pub slack_token: &'a str,
    pub slack_channel: &'a str,
    pub places:
        BTreeMap<std::string::String, BTreeMap<std::string::String, Vec<std::string::String>>>,
}

#[derive(Default)]
pub struct FeedItem {
    pub url: String,
    pub title: Option<String>,
    pub pub_date: Option<String>,
}

fn extract_llm_params<'a>(params: &'a ProcessItemParams<'a>) -> LLMParams {
    LLMParams {
        llm_client: params.llm_client.clone(),
        model: params.model.to_string(),
        temperature: params.temperature,
        require_json: None,
    }
}

pub async fn decision_loop(
    worker_id: i16,
    topics: &[String],
    llm_client: &LLMClient,
    model: &str,
    temperature: f32,
    slack_token: &str,
    slack_channel: &str,
) -> Result<()> {
    let db = Database::instance().await;
    let mut rng = StdRng::seed_from_u64(rand::random());

    let worker_detail = WorkerDetail {
        name: "decision worker".to_string(),
        id: worker_id,
        model: model.to_string(),
    };

    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: starting decision_loop using {:?}.", worker_detail.name, worker_detail.id, worker_detail.model, llm_client);

    // Each decision_worker loads places before entering the main loop.
    let places = match parse_places_data_hierarchical() {
        Ok(hierarchy) => hierarchy,
        Err(err) => panic!("Error: {}", err),
    };

    loop {
        // Determine which article to select next: 30% of the time seelct the newest
        // (latest news), 25% oldest (stale queue), 45% random.
        let roll = rng.random_range(0..100);
        let order = if roll < 30 {
            "newest"
        } else if roll < 55 {
            "oldest"
        } else {
            "random"
        };

        match db.fetch_and_delete_url_from_rss_queue(order).await {
            Ok(Some((url, title, pub_date))) => {
                if url.trim().is_empty() {
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping empty URL in queue.", worker_detail.name, worker_detail.id, worker_detail.model);
                    continue;
                }

                // Parse pub_date and check if it's older than 7 days
                let is_old_article = if let Some(date_str) = &pub_date {
                    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                        .ok()
                        .map(|date| {
                            let now = Utc::now().date_naive();
                            now.signed_duration_since(date) > ChronoDuration::days(7)
                            // Use ChronoDuration here
                        })
                        .unwrap_or(false)
                } else {
                    false
                };

                if is_old_article {
                    info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: skipping old article (published on {:?}): {}.", worker_detail.name, worker_detail.id, worker_detail.model, pub_date, url);

                    // Store it in the database to prevent reprocessing
                    let _ = db
                        .add_article(
                            &url,
                            false, // Not relevant
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            pub_date.as_deref(),
                        )
                        .await;
                    continue;
                }

                info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: loaded URL: {} ({:?}).", worker_detail.name, worker_detail.id, worker_detail.model, url, title);

                let item = FeedItem {
                    url: url.clone(),
                    title,
                    pub_date,
                };

                let places_clone = places.clone();

                let mut params = ProcessItemParams {
                    topics,
                    llm_client,
                    model,
                    temperature,
                    db,
                    slack_token,
                    slack_channel,
                    places: places_clone,
                };

                process_item(item, &mut params, &worker_detail).await;
            }
            Ok(None) => {
                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no URLs in queue, sleeping for 1 minute.", worker_detail.name, worker_detail.id, worker_detail.model);
                sleep(Duration::from_secs(60)).await;
                continue;
            }
            Err(e) => {
                error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error fetching URL from queue ({:?}), sleeping for 5 seconds.", worker_detail.name, worker_detail.id, worker_detail.model, e);
                sleep(Duration::from_secs(5)).await; // Wait and retry
                continue;
            }
        }
    }
}

pub async fn process_item(
    item: FeedItem,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) {
    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: reviewing {} ({}).", worker_detail.name, worker_detail.id, worker_detail.model, item.title.clone().unwrap_or_default(), item.url
    );
    let article_url = item.url;
    let article_title = item.title.unwrap_or_default();

    // Compute title_domain_hash
    let parsed_url = match Url::parse(&article_url) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!(
                target: TARGET_LLM_REQUEST,
                "Failed to parse article URL: {}: {}",
                article_url,
                e
            );
            return;
        }
    };
    let base_domain = parsed_url.domain().unwrap_or("");
    let title_domain_concat = format!("{}{}", base_domain, article_title);

    let mut hasher = Sha256::new();
    hasher.update(title_domain_concat.as_bytes());
    let title_domain_hash = format!("{:x}", hasher.finalize());

    // Check if this hash already exists in the database
    if params
        .db
        .has_title_domain_hash(&title_domain_hash)
        .await
        .unwrap_or(false)
    {
        info!(
            target: TARGET_LLM_REQUEST,
            "Article with title_domain_hash {} already processed, skipping.",
            title_domain_hash
        );
        return;
    }

    match extract_article_text(&article_url, worker_detail).await {
        Ok((article_text, article_html)) => {
            // Skip articles with no meaningful content
            if article_text.trim().is_empty() || article_text.trim().len() < 100 {
                warn!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Article '{}' has insufficient content, skipping.",
                    worker_detail.name, worker_detail.id, worker_detail.model, article_url
                );
                return;
            }

            let mut hasher = Sha256::new();
            hasher.update(article_text.as_bytes());
            let article_hash = format!("{:x}", hasher.finalize());

            // Check if the hash already exists in the database
            if params.db.has_hash(&article_hash).await.unwrap_or(false) {
                info!(target: TARGET_LLM_REQUEST, "Article with hash {} already processed, skipping.", article_hash);
                return;
            }

            let places = params.places.clone();

            let threat: String;
            if check_if_threat_at_all(&article_text, params, &worker_detail).await {
                threat =
                    determine_threat_location(&article_text, places, params, &worker_detail).await;
            } else {
                threat = "".to_string();
                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: article not about ongoing or imminent threat.", worker_detail.name, worker_detail.id, worker_detail.model);
            }

            if !threat.is_empty() {
                params
                    .db
                    .add_to_life_safety_queue(
                        &threat,
                        &article_url,
                        &article_title,
                        &article_text,
                        &article_html,
                        &article_hash,
                        &title_domain_hash,
                        item.pub_date.as_deref(),
                    )
                    .await
                    .unwrap_or_else(|e| {
                        error!(
                            target: TARGET_DB,
                            "Failed to add article to life safety queue: {:?}", e
                        )
                    });
            } else {
                process_topics(
                    &article_text,
                    &article_url,
                    &article_title,
                    &article_hash,
                    &title_domain_hash,
                    &article_html,
                    item.pub_date.as_deref(),
                    params,
                    worker_detail,
                )
                .await;
            }
            weighted_sleep().await;
        }
        Err(access_denied) => {
            handle_access_denied(
                access_denied,
                &article_url,
                &article_title,
                &&title_domain_hash,
                item.pub_date.as_deref(),
                params,
                worker_detail,
            )
            .await;
        }
    }
}

/// Checks if the article is about any kind of threat at all.
async fn check_if_threat_at_all(
    article_text: &str,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) -> bool {
    let llm_params = extract_llm_params(params);

    // Initial threat check
    let threat_prompt = prompts::threat_prompt(&article_text);
    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: asking LLM if about something affecting life or safety.", worker_detail.name, worker_detail.id, worker_detail.model);

    if let Some(response) = generate_llm_response(&threat_prompt, &llm_params, worker_detail).await
    {
        if response.trim().to_lowercase().starts_with("yes") {
            // Confirmation check
            let confirm_prompt = prompts::confirm_threat_prompt(&article_text);
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: confirming if genuine threat to life or safety.", worker_detail.name, worker_detail.id, worker_detail.model);

            if let Some(confirm_response) =
                generate_llm_response(&confirm_prompt, &llm_params, worker_detail).await
            {
                return confirm_response.trim().to_lowercase().starts_with("yes");
            }
        }
    }
    false
}

/// Processes the places mentioned in the article text and updates the affected people and places lists.
async fn determine_threat_location(
    article_text: &str,
    places: BTreeMap<std::string::String, BTreeMap<std::string::String, Vec<std::string::String>>>,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) -> String {
    // Generate the prompt based on the article text and places hierarchy
    let threat_locations_prompt = prompts::threat_locations(&article_text, &places);
    debug!(
        target: TARGET_LLM_REQUEST,
        "[{} {} {}]: asking LLM where threat is.",
        worker_detail.name,
        worker_detail.id,
        worker_detail.model
    );

    // Extract LLM parameters
    let llm_params = extract_llm_params(params);
    let mut json_llm_params = llm_params.clone();
    json_llm_params.require_json = Some(true);

    if let Some(response) =
        generate_llm_response(&threat_locations_prompt, &json_llm_params, worker_detail).await
    {
        info!("initial response: {}", response);
        let trimmed_response = response.trim();
        // Parse the JSON response
        info!("trimmed_response: {}", trimmed_response);

        match serde_json::from_str::<ThreatLocationResponse>(trimmed_response) {
            Ok(json_response) => {
                info!("json_response: {:?}", json_response);

                // Check if any region is impacted
                if json_response.impacted_regions.iter().any(|region| {
                    let continent = region.continent.as_deref().unwrap_or("");
                    let country = region.country.as_deref().unwrap_or("");
                    let region_name = region.region.as_deref().unwrap_or("");
                    places.iter().any(|(c, countries)| {
                        c == continent
                            || countries.iter().any(|(co, regions)| {
                                co == country || regions.iter().any(|r| r == region_name)
                            })
                    })
                }) {
                    return trimmed_response.to_string();
                }
            }
            Err(e) => {
                warn!("Failed to parse JSON response: {}", e);
            }
        }
    }

    // If no region is impacted, return an empty string
    "".to_string()
}

async fn article_is_relevant(
    article_text: &str,
    topic_prompt: &str,
    pub_date: Option<&str>,
    llm_params: &mut LLMParams,
    worker_detail: &WorkerDetail,
) -> bool {
    // Be sure content has sufficient content.
    if article_text.split_whitespace().count() < 50 {
        debug!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {}]: Article has fewer than 50 words, skipping relevance check.",
            worker_detail.name, worker_detail.id, worker_detail.model
        );
        return false;
    }

    // Generate summary
    let summary_prompt = prompts::summary_prompt(article_text, pub_date);
    let summary_response = generate_llm_response(&summary_prompt, &llm_params, worker_detail)
        .await
        .unwrap_or_default();

    // Confirm the article relevance
    let confirm_prompt = prompts::confirm_prompt(&summary_response, topic_prompt);
    if let Some(confirm_response) =
        generate_llm_response(&confirm_prompt, &llm_params, worker_detail).await
    {
        if confirm_response.trim().to_lowercase().starts_with("yes") {
            return true;
        }
    }
    return false;
}

/// Processes the topics mentioned in the article text and sends the results to the Slack channel if relevant.
async fn process_topics(
    article_text: &str,
    article_url: &str,
    article_title: &str,
    article_hash: &str,
    title_domain_hash: &str,
    article_html: &str,
    pub_date: Option<&str>,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) {
    let mut article_relevant = false;

    for topic in params.topics {
        let parts: Vec<_> = topic.trim().split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let topic_name = parts[0].trim();
        let topic_prompt = parts[1].trim();

        if topic_name.is_empty() || topic_prompt.is_empty() {
            continue;
        }

        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: asking if about {}: {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, topic_prompt);

        let yes_no_prompt = prompts::is_this_about(article_text, topic_prompt);
        let mut llm_params = extract_llm_params(params);
        if let Some(yes_no_response) =
            generate_llm_response(&yes_no_prompt, &llm_params, worker_detail).await
        {
            if yes_no_response.trim().to_lowercase().starts_with("yes") {
                // Article is relevant to the topic
                article_relevant = true;

                // Perform a secondary check before posting to Slack
                if params.db.has_hash(&article_hash).await.unwrap_or(false) {
                    info!(
                        target: TARGET_LLM_REQUEST,
                        "Article with hash {} was already processed (second check), skipping Slack post for topic '{}'.",
                        article_hash,
                        topic_name
                    );
                    continue; // Skip to the next topic
                }

                if article_is_relevant(
                    article_text,
                    topic_prompt,
                    pub_date,
                    &mut llm_params,
                    worker_detail,
                )
                .await
                {
                    // Add to matched topics queue
                    if let Err(e) = params
                        .db
                        .add_to_matched_topics_queue(
                            article_text,
                            article_html,
                            article_url,
                            article_title,
                            article_hash,
                            title_domain_hash,
                            topic_name,
                            pub_date,
                        )
                        .await
                    {
                        error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: failed to add to Matched Topics queue: {}: [{:?}].", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, e);
                    } else {
                        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: added to Matched Topics queue: {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name);
                    }

                    return; // No need to continue checking other topics
                } else {
                    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: not about '{}' or is promotional.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name);
                    weighted_sleep().await;
                }
            } else {
                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: not about '{}': {}.", worker_detail.name, worker_detail.id, worker_detail.model, topic_name, yes_no_response.trim());
                weighted_sleep().await;
            }
        }
    }

    // If no relevant topic was found, add the URL to the database as a non-relevant article
    if !article_relevant {
        match params
            .db
            .add_article(
                article_url,
                false,
                None,
                None,
                None,
                Some(&article_hash),
                Some(&title_domain_hash),
                None,
                pub_date,
            )
            .await
        {
            Ok(_) => {
                debug!(target: TARGET_DB, "[{} {} {}]: added non-relevant article to database.", worker_detail.name, worker_detail.id, worker_detail.model);
            }
            Err(e) => {
                debug!(target: TARGET_DB, "[{} {} {}]: failed to add non-relevant article to database: {:?}", worker_detail.name, worker_detail.id, worker_detail.model, e);
            }
        }
    }
}

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
async fn extract_article_text(
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
async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    article_title: &str,
    title_domain_hash: &str,
    pub_date: Option<&str>,
    params: &mut ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) {
    if access_denied {
        match params
            .db
            .add_article(
                article_url,
                false,
                None,
                None,
                None,
                None,
                Some(&title_domain_hash),
                None,
                pub_date,
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
