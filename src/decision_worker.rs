use rand::{rngs::StdRng, Rng, SeedableRng};
use readability::extractor;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::prompts;
use crate::util::weighted_sleep;
use crate::{LLMClient, LLMParams};
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
    pub places: Option<Value>,
}

/// Parameters required for processing places, including article text and affected people and places.
struct PlaceProcessingParams<'a> {
    article_text: &'a str,
    affected_regions: &'a mut BTreeSet<String>,
    affected_people: &'a mut BTreeSet<String>,
    affected_places: &'a mut BTreeSet<String>,
    non_affected_people: &'a mut BTreeSet<String>,
    non_affected_places: &'a mut BTreeSet<String>,
}

/// Parameters required for processing a region, including article text and affected people and places.
struct RegionProcessingParams<'a> {
    article_text: &'a str,
    country: &'a str,
    continent: &'a str,
    region: &'a str,
    cities: &'a serde_json::Value,
    affected_regions: &'a mut BTreeSet<String>,
    affected_people: &'a mut BTreeSet<String>,
    affected_places: &'a mut BTreeSet<String>,
    non_affected_people: &'a mut BTreeSet<String>,
    non_affected_places: &'a mut BTreeSet<String>,
}

/// Parameters required for processing a city, including article text and affected people and places.
struct CityProcessingParams<'a> {
    article_text: &'a str,
    city_name: &'a str,
    region: &'a str,
    country: &'a str,
    continent: &'a str,
    city_data: &'a [&'a str],
    affected_people: &'a mut BTreeSet<String>,
    affected_places: &'a mut BTreeSet<String>,
}

#[derive(Default)]
pub struct FeedItem {
    pub url: String,
    pub title: Option<String>,
}

fn extract_llm_params<'a>(params: &'a ProcessItemParams<'a>) -> LLMParams {
    LLMParams {
        llm_client: params.llm_client.clone(),
        model: params.model.to_string(),
        temperature: params.temperature,
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
    places: Option<Value>,
) {
    let db = Database::instance().await;
    let mut rng = StdRng::from_entropy();

    info!(target: TARGET_LLM_REQUEST, "Decision worker {}: starting decision_loop.", worker_id);
    debug!(
        "Decision worker {} is running with model '{}' using {:?}.",
        worker_id, model, llm_client
    );

    loop {
        let roll = rng.gen_range(0..100);
        let order = if roll < 30 {
            "newest"
        } else if roll < 55 {
            "oldest"
        } else {
            "random"
        };

        match db.fetch_and_delete_url_from_rss_queue(order).await {
            Ok(Some((url, title))) => {
                if url.trim().is_empty() {
                    error!(target: TARGET_LLM_REQUEST, "decision worker {}: Found an empty URL in the queue, skipping...", worker_id);
                    continue;
                }

                info!(target: TARGET_LLM_REQUEST, "decision worker {}: Moving on to a new URL: {} ({:?})", worker_id, url, title);

                let item = FeedItem {
                    url: url.clone(),
                    title,
                };

                let mut params = ProcessItemParams {
                    topics,
                    llm_client,
                    model,
                    temperature,
                    db,
                    slack_token,
                    slack_channel,
                    places: places.clone(),
                };

                process_item(item, &mut params).await;
            }
            Ok(None) => {
                debug!(target: TARGET_LLM_REQUEST, "decision worker {}: No URLs to process. Sleeping for 1 minute before retrying.", worker_id);
                sleep(Duration::from_secs(60)).await;
                continue;
            }
            Err(e) => {
                error!(target: TARGET_LLM_REQUEST, "decision worker {}: Error fetching URL from queue: {:?}", worker_id, e);
                sleep(Duration::from_secs(5)).await; // Wait and retry
                continue;
            }
        }
    }
}

pub async fn process_item(item: FeedItem, params: &mut ProcessItemParams<'_>) {
    let worker_id = format!("{:?}", std::thread::current().id());

    debug!(
        target: TARGET_LLM_REQUEST,
        "decision worker {}: Reviewing => {} ({})",
        worker_id,
        item.title.clone().unwrap_or_default(),
        item.url
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

    match extract_article_text(&article_url).await {
        Ok((article_text, article_html)) => {
            let mut hasher = Sha256::new();
            hasher.update(article_text.as_bytes());
            let article_hash = format!("{:x}", hasher.finalize());

            // Check if the hash already exists in the database
            if params.db.has_hash(&article_hash).await.unwrap_or(false) {
                info!(target: TARGET_LLM_REQUEST, "Article with hash {} already processed, skipping.", article_hash);
                return;
            }

            let mut affected_regions = BTreeSet::new();
            let mut affected_people = BTreeSet::new();
            let mut affected_places = BTreeSet::new();
            let mut non_affected_people = BTreeSet::new();
            let mut non_affected_places = BTreeSet::new();

            let places = params.places.clone();

            if let Some(places) = places {
                let place_params = PlaceProcessingParams {
                    article_text: &article_text,
                    affected_regions: &mut affected_regions,
                    affected_people: &mut affected_people,
                    affected_places: &mut affected_places,
                    non_affected_people: &mut non_affected_people,
                    non_affected_places: &mut non_affected_places,
                };

                if check_if_threat_at_all(&article_text, params).await {
                    process_places(place_params, &places, params).await;
                } else {
                    debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Article is not about an ongoing or imminent threat.", worker_id);
                }
            }

            if !affected_people.is_empty() || !non_affected_people.is_empty() {
                summarize_and_send_article(
                    &article_url,
                    &article_title,
                    &article_text,
                    &affected_regions,
                    &affected_people,
                    &affected_places,
                    &non_affected_people,
                    &non_affected_places,
                    &article_hash,
                    &title_domain_hash,
                    &article_html,
                    params,
                )
                .await;
            } else {
                process_topics(
                    &article_text,
                    &article_url,
                    &article_title,
                    &article_hash,
                    &title_domain_hash,
                    &article_html,
                    params,
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
                params,
            )
            .await;
        }
    }
}

/// Checks if the article is about any kind of threat at all.
async fn check_if_threat_at_all(article_text: &str, params: &mut ProcessItemParams<'_>) -> bool {
    let threat_prompt = prompts::threat_prompt(&article_text);
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Asking LLM: is this article about an ongoing or imminent and potentially life-threatening event", worker_id);

    let llm_params = extract_llm_params(params);
    match generate_llm_response(&threat_prompt, &llm_params).await {
        Some(response) => response.trim().to_lowercase().starts_with("yes"),
        None => false,
    }
}

/// Processes the places mentioned in the article text and updates the affected people and places lists.
async fn process_places(
    mut place_params: PlaceProcessingParams<'_>,
    places: &serde_json::Value,
    params: &mut ProcessItemParams<'_>,
) {
    let worker_id = format!("{:?}", std::thread::current().id());
    let mut article_about_any_area = false;
    for (continent, countries) in places.as_object().unwrap() {
        if !process_continent(&mut place_params, continent, countries, params).await {
            article_about_any_area = true;
            debug!(
                target: TARGET_LLM_REQUEST,
                "decision worker {}: Article is not about something affecting life or safety on '{}'",
                worker_id,
                continent
            );
        }
        weighted_sleep().await;
    }

    if !article_about_any_area {
        // The article is not about any areas of interest; clear non_affected_people
        place_params.non_affected_people.clear();
        place_params.non_affected_places.clear();
    }
}

/// Processes the continent data and updates the affected people and places lists.
async fn process_continent(
    place_params: &mut PlaceProcessingParams<'_>,
    continent: &str,
    countries: &serde_json::Value,
    params: &mut ProcessItemParams<'_>,
) -> bool {
    let PlaceProcessingParams {
        article_text,
        affected_regions,
        affected_people,
        affected_places,
        non_affected_people,
        non_affected_places,
    } = place_params;

    let continent_prompt = prompts::continent_threat_prompt(article_text, continent);
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Asking LLM: is this article about ongoing or imminent threat on {}", worker_id, continent);

    let llm_params = extract_llm_params(params);
    let continent_response = match generate_llm_response(&continent_prompt, &llm_params).await {
        Some(response) => response,
        None => return false,
    };

    if !continent_response.trim().to_lowercase().starts_with("yes") {
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "decision worker {}: Article is about something affecting life or safety on '{}'",
        worker_id,
        continent
    );

    for (country, regions) in countries.as_object().unwrap() {
        if process_country(
            article_text,
            country,
            continent,
            regions,
            affected_regions,
            affected_people,
            affected_places,
            non_affected_people,
            non_affected_places,
            params,
        )
        .await
        {
            return true;
        }
    }

    true
}

/// Processes the country data and updates the affected people and places lists.
async fn process_country(
    article_text: &str,
    country: &str,
    continent: &str,
    regions: &serde_json::Value,
    affected_regions: &mut BTreeSet<String>,
    affected_people: &mut BTreeSet<String>,
    affected_places: &mut BTreeSet<String>,
    non_affected_people: &mut BTreeSet<String>,
    non_affected_places: &mut BTreeSet<String>,
    params: &mut ProcessItemParams<'_>,
) -> bool {
    let country_prompt = prompts::country_threat_prompt(article_text, country, continent);
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Asking LLM: is this article about ongoing or imminent threat in {} on {}", worker_id, country, continent);

    let llm_params = extract_llm_params(params);
    let country_response = match generate_llm_response(&country_prompt, &llm_params).await {
        Some(response) => response,
        None => return false,
    };

    if !country_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "decision worker {}: Article is not about something affecting life or safety in '{}' on '{}'",
            worker_id,
            country,
            continent
        );
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "decision worker {}: Article is about something affecting life or safety in '{}' on '{}'",
        worker_id,
        country,
        continent
    );

    for (region, cities) in regions.as_object().unwrap() {
        let region_params = RegionProcessingParams {
            article_text,
            country,
            continent,
            region,
            cities,
            affected_regions,
            affected_people,
            affected_places,
            non_affected_people,
            non_affected_places,
        };

        if process_region(region_params, params).await {
            return true;
        }
    }

    true
}

/// Processes the region data and updates the affected people and places lists.
async fn process_region(
    params: RegionProcessingParams<'_>,
    proc_params: &mut ProcessItemParams<'_>,
) -> bool {
    let RegionProcessingParams {
        article_text,
        country,
        continent,
        region,
        cities,
        affected_regions,
        affected_people,
        affected_places,
        non_affected_people,
        non_affected_places,
    } = params;

    let region_prompt = prompts::region_threat_prompt(article_text, region, country, continent);
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Asking LLM: is this article about ongoing or imminent threat in {} in {} on {}", worker_id, region, country, continent);

    let llm_params = extract_llm_params(proc_params);
    let region_response = match generate_llm_response(&region_prompt, &llm_params).await {
        Some(response) => response,
        None => return false,
    };

    if !region_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "decision worker {}: Article is not about something affecting life or safety in '{}', '{}'",
            worker_id,
            region,
            country
        );
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "decision worker {}: Article is about something affecting life or safety in '{}', '{}'",
        worker_id,
        region,
        country
    );
    affected_regions.insert(region.to_string());

    for city in cities.as_array().unwrap() {
        let city_data: Vec<&str> = city.as_str().unwrap().split(", ").collect();
        let city_name = city_data[2];

        let city_params = CityProcessingParams {
            article_text,
            city_name,
            region,
            country,
            continent,
            city_data: &city_data,
            affected_people,
            affected_places,
        };

        if process_city(city_params, proc_params).await {
            // Remember affected city
            affected_people.insert(format!(
                "{} {} ({}) in {}",
                city_data[0], city_data[1], city_data[5], city_name
            ));
            affected_places.insert(format!("{} in {} on {}", city_name, country, continent));
        } else {
            // Remember non-affected city
            non_affected_people.insert(format!(
                "{} {} ({}) in {}",
                city_data[0], city_data[1], city_data[5], city_name
            ));
            non_affected_places.insert(format!("{} in {} on {}", city_name, country, continent));
        }
    }

    true
}

/// Processes the city data and updates the affected people and places lists.
async fn process_city(
    params: CityProcessingParams<'_>,
    proc_params: &mut ProcessItemParams<'_>,
) -> bool {
    let CityProcessingParams {
        article_text,
        city_name,
        region,
        country,
        continent,
        city_data,
        affected_people,
        affected_places,
    } = params;

    let city_prompt =
        prompts::city_threat_prompt(article_text, city_name, region, country, continent);
    let worker_id = format!("{:?}", std::thread::current().id());
    let llm_params = extract_llm_params(proc_params);
    let city_response = match generate_llm_response(&city_prompt, &llm_params).await {
        Some(response) => response,
        None => return false,
    };

    if !city_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "decision worker {}: Article is not about something affecting life or safety in '{}, {}, {}'",
            worker_id,
            city_name,
            region,
            country
        );
        return false;
    }

    info!(
        target: TARGET_LLM_REQUEST,
        "decision worker {}: Article is about something affecting life or safety in '{}, {}, {}'",
        worker_id,
        city_name,
        region,
        country
    );

    affected_people.insert(format!(
        "{} {} ({}) in {}",
        city_data[0], city_data[1], city_data[5], city_name
    ));
    affected_places.insert(format!("{} in {} on {}", city_name, country, continent));

    true
}

async fn article_is_relevant(
    article_text: &str,
    topic_name: &str,
    llm_params: &mut LLMParams,
) -> bool {
    // Generate summary
    let summary_prompt = prompts::summary_prompt(article_text);
    let summary_response = generate_llm_response(&summary_prompt, &llm_params)
        .await
        .unwrap_or_default();

    // Confirm the article relevance
    let confirm_prompt = prompts::confirm_prompt(&summary_response, topic_name);
    if let Some(confirm_response) = generate_llm_response(&confirm_prompt, &llm_params).await {
        if confirm_response.trim().to_lowercase().starts_with("yes") {
            return true;
        }
    }
    return false;
}

/// Summarizes and sends the article to the Slack channel, and updates the database with the article data.
async fn summarize_and_send_article(
    article_url: &str,
    article_title: &str,
    article_text: &str,
    affected_regions: &BTreeSet<String>,
    affected_people: &BTreeSet<String>,
    affected_places: &BTreeSet<String>,
    non_affected_people: &BTreeSet<String>,
    non_affected_places: &BTreeSet<String>,
    article_hash: &str,
    title_domain_hash: &str,
    article_html: &str,
    params: &mut ProcessItemParams<'_>,
) {
    let mut llm_params = extract_llm_params(params);

    if article_is_relevant(
        article_text,
        "an ongoing or imminent life-threatening event",
        &mut llm_params,
    )
    .await
    {
        params
            .db
            .add_to_life_safety_queue(
                article_url,
                article_title,
                article_text,
                article_html,
                article_hash,
                title_domain_hash,
                affected_regions,
                affected_people,
                affected_places,
                non_affected_people,
                non_affected_places,
            )
            .await
            .unwrap_or_else(|e| {
                error!(
                    target: TARGET_DB,
                    "Failed to add article to life safety queue: {:?}", e
                )
            });
    }
}

/// Processes the topics mentioned in the article text and sends the results to the Slack channel if relevant.
async fn process_topics(
    article_text: &str,
    article_url: &str,
    article_title: &str,
    article_hash: &str,
    title_domain_hash: &str,
    article_html: &str,
    params: &mut ProcessItemParams<'_>,
) {
    let worker_id = format!("{:?}", std::thread::current().id());
    let mut article_relevant = false;

    for topic in params.topics {
        let parts: Vec<_> = topic.trim().split(':').collect();
        if parts.is_empty() {
            continue;
        }
        let topic_name = parts[0];
        if topic_name.is_empty() {
            continue;
        }

        debug!(target: TARGET_LLM_REQUEST, "decision worker {}: Asking LLM: is this article specifically about {}", worker_id, topic_name);

        let yes_no_prompt = prompts::is_this_about(article_text, topic_name);
        let mut llm_params = extract_llm_params(params);
        if let Some(yes_no_response) = generate_llm_response(&yes_no_prompt, &llm_params).await {
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

                if article_is_relevant(article_text, topic_name, &mut llm_params).await {
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
                        )
                        .await
                    {
                        error!(target: TARGET_DB, "Failed to add to matched topics queue: {:?}", e);
                    } else {
                        debug!(
                            target: TARGET_DB,
                            "decision worker {}: Successfully added to matched topics queue: topic '{}'",
                            worker_id,
                            topic_name
                        );
                    }

                    return; // No need to continue checking other topics
                } else {
                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "decision worker {}: Article is not about '{}' or is a promotion/",
                        worker_id,
                        topic_name,
                    );
                    weighted_sleep().await;
                }
            } else {
                debug!(
                    target: TARGET_LLM_REQUEST,
                    "decision worker {}: Article is not about '{}': {}",
                    worker_id,
                    topic_name,
                    yes_no_response.trim()
                );
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
            )
            .await
        {
            Ok(_) => {
                debug!(target: TARGET_DB, "decision worker {}: Successfully added non-relevant article to database", worker_id)
            }
            Err(e) => {
                error!(target: TARGET_DB, "decision worker {}: Failed to add non-relevant article to database: {:?}", worker_id, e)
            }
        }
    }
}

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
async fn extract_article_text(url: &str) -> Result<(String, String), bool> {
    let max_retries = 3;
    let article_text: String;
    let article_html: String;
    let mut backoff = 2;
    let worker_id = format!("{:?}", std::thread::current().id());

    for retry_count in 0..max_retries {
        let scrape_future = async { extractor::scrape(url) };
        debug!(target: TARGET_WEB_REQUEST, "decision worker {}: Requesting extraction for URL: {}", worker_id, url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    warn!(target: TARGET_WEB_REQUEST, "decision worker {}: Extracted article is empty for URL: {}", worker_id, url);
                    break;
                }
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                article_html = product.content.clone();

                debug!(target: TARGET_WEB_REQUEST, "decision worker {}: Extraction succeeded for URL: {}", worker_id, url);
                return Ok((article_text, article_html));
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "decision worker {}: Error extracting page: {:?}", worker_id, e);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "decision worker {}: Retrying... ({}/{})", worker_id, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "decision worker {}: Failed to extract article after {} retries", worker_id, max_retries);
                }
                if e.to_string().contains("Access Denied") || e.to_string().contains("Unexpected") {
                    return Err(true);
                }
            }
            Err(_) => {
                warn!(target: TARGET_WEB_REQUEST, "decision worker {}: Operation timed out", worker_id);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "decision worker {}: Retrying... ({}/{})", worker_id, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "decision worker {}: Failed to extract article after {} retries", worker_id, max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }

    warn!(target: TARGET_WEB_REQUEST, "decision worker {}: Article text extraction failed for URL: {}", worker_id, url);
    Err(false)
}

/// Handles the case where access to the article is denied, updating the database and logging a warning.
async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    article_title: &str,
    title_domain_hash: &str,
    params: &mut ProcessItemParams<'_>,
) {
    let worker_id = format!("{:?}", std::thread::current().id());
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
            )
            .await
        {
            Ok(_) => {
                warn!(target: TARGET_WEB_REQUEST, "decision worker {}: Access denied for URL: {} ({})", worker_id, article_url, article_title)
            }
            Err(e) => {
                error!(target: TARGET_WEB_REQUEST, "decision worker {}: Failed to add access denied URL '{}' ({}) to database: {:?}", worker_id, article_url, article_title, e)
            }
        }
    }
}