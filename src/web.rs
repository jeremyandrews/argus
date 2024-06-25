use ollama_rs::Ollama;
use readability::extractor;
use rss::Channel;
use serde_json::Value;
use std::io;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::slack::send_to_slack;
use crate::TARGET_WEB_REQUEST;

/// Parameters required for processing an item, including topics, database, and Slack channel information.
pub struct ProcessItemParams<'a> {
    /// List of topics to analyze.
    pub topics: &'a [String],
    /// Ollama client for generating responses.
    pub ollama: &'a Ollama,
    /// Model name to use for generating responses.
    pub model: &'a str,
    /// Temperature setting for generating responses.
    pub temperature: f32,
    /// Mutable reference to the database for storing and retrieving data.
    pub db: &'a mut Database,
    /// Slack token for authentication.
    pub slack_token: &'a str,
    /// Slack channel ID to send messages to.
    pub slack_channel: &'a str,
    /// Optional JSON value containing places data.
    pub places: Option<Value>,
    /// Mutable reference to a vector of strings to store the non-affected people.
    pub non_affected_people: &'a mut Vec<String>,
    /// Mutable reference to a vector of strings to store the non-affected places.
    pub non_affected_places: &'a mut Vec<String>,
}

/// Parameters required for processing a region, including article text and affected people and places.
struct RegionProcessingParams<'a> {
    article_text: &'a str,
    country: &'a str,
    continent: &'a str,
    region: &'a str,
    cities: &'a serde_json::Value,
    affected_people: &'a mut Vec<String>,
    affected_places: &'a mut Vec<String>,
    non_affected_people: &'a mut Vec<String>,
    non_affected_places: &'a mut Vec<String>,
}

/// Parameters required for processing a city, including article text and affected people and places.
struct CityProcessingParams<'a> {
    /// Text of the article being processed.
    article_text: &'a str,
    /// Name of the city.
    city_name: &'a str,
    /// Name of the region.
    region: &'a str,
    /// Name of the country.
    country: &'a str,
    /// Name of the continent.
    continent: &'a str,
    /// Data related to the city.
    city_data: &'a [&'a str],
    /// Mutable reference to a vector of strings to store the affected people.
    affected_people: &'a mut Vec<String>,
    /// Mutable reference to a vector of strings to store the affected places.
    affected_places: &'a mut Vec<String>,
}

/// Processes a list of URLs by fetching and parsing RSS feeds, extracting and analyzing articles, and updating the database and Slack channel with the results.
///
/// # Arguments
///
/// * `urls` - A vector of URLs to process.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - An Ok result if the processing succeeds, or an error if it fails.
pub async fn process_urls(
    urls: Vec<String>,
    params: &mut ProcessItemParams<'_>,
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

/// Processes a single RSS item by extracting and analyzing the article text, and updating the affected people and places lists.
///
/// # Arguments
///
/// * `item` - The RSS item to process.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
async fn process_item(item: rss::Item, params: &mut ProcessItemParams<'_>) {
    info!(
        " - reviewing => {} ({})",
        item.title.clone().unwrap_or_default(),
        item.link.clone().unwrap_or_default()
    );
    let article_url = item.link.clone().unwrap_or_default();

    match extract_article_text(&article_url).await {
        Ok(article_text) => {
            let mut affected_people = Vec::new();
            let mut affected_places = Vec::new();
            let mut non_affected_people = Vec::new();
            let mut non_affected_places = Vec::new();

            // Extract places from params
            let places = params.places.clone();

            if let Some(places) = places {
                process_places(
                    &article_text,
                    &places,
                    &mut affected_people,
                    &mut affected_places,
                    &mut non_affected_people,
                    &mut non_affected_places,
                    params,
                )
                .await;
            }

            if !affected_people.is_empty() || !non_affected_people.is_empty() {
                summarize_and_send_article(
                    &article_url,
                    &item,
                    &article_text,
                    &affected_people,
                    &affected_places,
                    &non_affected_people,
                    &non_affected_places,
                    params,
                )
                .await;
            } else {
                process_topics(&article_text, &article_url, &item, params).await;
            }
            debug!(" zzz - sleeping 60 seconds ...");
            sleep(Duration::from_secs(60)).await;
        }
        Err(access_denied) => handle_access_denied(access_denied, &article_url, params).await,
    }
}

/// Processes the places mentioned in the article text and updates the affected people and places lists.
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `places` - The JSON value containing the places data.
/// * `affected_people` - A mutable reference to a vector of strings to store the affected people.
/// * `affected_places` - A mutable reference to a vector of strings to store the affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
/// Processes the places mentioned in the article text and updates the affected people and places lists.
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `places` - The JSON value containing the places data.
/// * `affected_people` - A mutable reference to a vector of strings to store the affected people.
/// * `affected_places` - A mutable reference to a vector of strings to store the affected places.
/// * `non_affected_people` - A mutable reference to a vector of strings to store the non-affected people.
/// * `non_affected_places` - A mutable reference to a vector of strings to store the non-affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
async fn process_places(
    article_text: &str,
    places: &serde_json::Value,
    affected_people: &mut Vec<String>,
    affected_places: &mut Vec<String>,
    non_affected_people: &mut Vec<String>,
    non_affected_places: &mut Vec<String>,
    params: &mut ProcessItemParams<'_>,
) {
    for (continent, countries) in places.as_object().unwrap() {
        if !process_continent(
            article_text,
            continent,
            countries,
            affected_people,
            affected_places,
            non_affected_people,
            non_affected_places,
            params,
        )
        .await
        {
            info!(
                "Article is not about something affecting life or safety on '{}'",
                continent
            );
        }
    }
}

/// Processes the continent data and updates the affected people and places lists.
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `continent` - The name of the continent.
/// * `countries` - The JSON value containing the countries data.
/// * `affected_people` - A mutable reference to a vector of strings to store the affected people.
/// * `affected_places` - A mutable reference to a vector of strings to store the affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
///
/// # Returns
///
/// * `bool` - `true` if the continent data was processed successfully, otherwise `false`.
async fn process_continent(
    article_text: &str,
    continent: &str,
    countries: &serde_json::Value,
    affected_people: &mut Vec<String>,
    affected_places: &mut Vec<String>,
    non_affected_people: &mut Vec<String>,
    non_affected_places: &mut Vec<String>,
    params: &mut ProcessItemParams<'_>,
) -> bool {
    let continent_prompt = format!(
        "{} | Is this a significant event affecting life and safety of people living on the continent of {} in the past weeks? Answer yes or no.",
        article_text, continent
    );

    let continent_response = match generate_llm_response(&continent_prompt, params).await {
        Some(response) => response,
        None => return false,
    };

    if !continent_response.trim().to_lowercase().starts_with("yes") {
        return false;
    }

    for (country, regions) in countries.as_object().unwrap() {
        if process_country(
            article_text,
            country,
            continent,
            regions,
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
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `country` - The name of the country.
/// * `continent` - The name of the continent.
/// * `regions` - The JSON value containing the regions data.
/// * `affected_people` - A mutable reference to a vector of strings to store the affected people.
/// * `affected_places` - A mutable reference to a vector of strings to store the affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
///
/// # Returns
///
/// * `bool` - `true` if the country data was processed successfully, otherwise `false`.
async fn process_country(
    article_text: &str,
    country: &str,
    continent: &str,
    regions: &serde_json::Value,
    affected_people: &mut Vec<String>,
    affected_places: &mut Vec<String>,
    non_affected_people: &mut Vec<String>,
    non_affected_places: &mut Vec<String>,
    params: &mut ProcessItemParams<'_>,
) -> bool {
    for (region, cities) in regions.as_object().unwrap() {
        let region_params = RegionProcessingParams {
            article_text,
            country,
            continent,
            region,
            cities,
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
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `country` - The name of the country.
/// * `continent` - The name of the continent.
/// * `region` - The name of the region.
/// * `cities` - The JSON value containing the cities data.
/// * `affected_people` - A mutable reference to a vector of strings to store the affected people.
/// * `affected_places` - A mutable reference to a vector of strings to store the affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
///
/// # Returns
///
/// * `bool` - `true` if the region data was processed successfully, otherwise `false`.
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
        affected_people,
        affected_places,
        non_affected_people,
        non_affected_places,
    } = params;

    let region_prompt = format!(
        "{} | Is this a significant event affecting life and safety of people living in the region of {} in the country of {} on {} in the past weeks? Answer yes or no.",
        article_text, region, country, continent
    );

    let region_response = match generate_llm_response(&region_prompt, proc_params).await {
        Some(response) => response,
        None => return false,
    };

    if !region_response.trim().to_lowercase().starts_with("yes") {
        info!(
            "Article is not about something affecting life or safety in '{}', '{}'",
            region, country
        );
        return false;
    }

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
            affected_people.push(format!(
                "{} {} ({}) in {}",
                city_data[0], city_data[1], city_data[5], city_name
            ));
            affected_places.push(format!("{} in {} on {}", city_name, country, continent));
        } else {
            // Remember non-affected city
            non_affected_people.push(format!(
                "{} {} ({}) in {}",
                city_data[0], city_data[1], city_data[5], city_name
            ));
            non_affected_places.push(format!("{} in {} on {}", city_name, country, continent));
        }
    }

    true
}

/// Processes the city data and updates the affected people and places lists.
///
/// # Arguments
///
/// * `params` - A `CityProcessingParams` struct containing the necessary parameters for processing the city.
/// * `proc_params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
///
/// # Returns
///
/// * `bool` - `true` if the city data was processed successfully, otherwise `false`.
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

    let city_prompt = format!(
        "{} | Is this a significant event affecting life and safety of people living in or near the city of {} in the region of {} in the country of {} on {} in the past weeks? Answer yes or no.",
        article_text, city_name, region, country, continent
    );

    let city_response = match generate_llm_response(&city_prompt, proc_params).await {
        Some(response) => response,
        None => return false,
    };

    if !city_response.trim().to_lowercase().starts_with("yes") {
        info!(
            "Article is not about something affecting life or safety in '{}, {}, {}'",
            city_name, region, country
        );
        return false;
    }

    affected_people.push(format!(
        "{} {} ({}) in {}",
        city_data[0], city_data[1], city_data[5], city_name
    ));
    affected_places.push(format!("{} in {} on {}", city_name, country, continent));

    true
}

/// Summarizes and sends the article to the Slack channel, and updates the database with the article data.
///
/// # Arguments
///
/// * `article_url` - The URL of the article.
/// * `item` - The RSS item.
/// * `article_text` - The text of the article.
/// * `affected_people` - A slice of strings containing the affected people.
/// * `affected_places` - A slice of strings containing the affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
/// Summarizes and sends the article to the Slack channel, and updates the database with the article data.
///
/// # Arguments
///
/// * `article_url` - The URL of the article.
/// * `item` - The RSS item.
/// * `article_text` - The text of the article.
/// * `affected_people` - A slice of strings containing the affected people.
/// * `affected_places` - A slice of strings containing the affected places.
/// * `non_affected_people` - A slice of strings containing the non-affected people.
/// * `non_affected_places` - A slice of strings containing the non-affected places.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
async fn summarize_and_send_article(
    article_url: &str,
    item: &rss::Item,
    article_text: &str,
    affected_people: &[String],
    affected_places: &[String],
    non_affected_people: &[String],
    non_affected_places: &[String],
    params: &mut ProcessItemParams<'_>,
) {
    let formatted_article = format!(
        "*<{}|{}>*",
        article_url,
        item.title.clone().unwrap_or_default()
    );

    let mut full_message = String::new();

    if !affected_people.is_empty() {
        let affected_summary = format!("This article affects: {}", affected_people.join(", "));
        let summary_prompt = format!(
            "Summarize the following article in a couple paragraphs, and provide a one paragraph critical analysis:\n\n{}",
            article_text
        );
        let article_summary = generate_llm_response(&summary_prompt, params)
            .await
            .unwrap_or_default();

        let how_prompt = format!(
            "{} | How does this article affect the life and safety of people living in the following places: {}? Answer in a few sentences.",
            article_text,
            affected_places.join(", ")
        );
        let how_response = generate_llm_response(&how_prompt, params)
            .await
            .unwrap_or_default();

        full_message.push_str(&format!(
            "{}\n\n{}\n\n{}",
            affected_summary, article_summary, how_response
        ));
    }

    if !non_affected_people.is_empty() {
        let non_affected_summary = format!(
            "This article does not affect: {}",
            non_affected_people.join(", ")
        );
        let why_not_prompt = format!(
            "{} | Why does this article not affect the life and safety of people living in the following places: {}? Answer in a few sentences.",
            article_text,
            non_affected_places.join(", ")
        );
        let why_not_response = generate_llm_response(&why_not_prompt, params)
            .await
            .unwrap_or_default();

        if !full_message.is_empty() {
            full_message.push_str("\n\n");
        }
        full_message.push_str(&format!("{}\n\n{}", non_affected_summary, why_not_response));
    }

    if !full_message.is_empty() {
        send_to_slack(
            &formatted_article,
            &full_message,
            params.slack_token,
            params.slack_channel,
        )
        .await;
        params
            .db
            .add_article(article_url, true, None, Some(&full_message))
            .expect("Failed to add article to database");
    }
}

/// Processes the topics mentioned in the article text and sends the results to the Slack channel if relevant.
///
/// # Arguments
///
/// * `article_text` - The text of the article.
/// * `article_url` - The URL of the article.
/// * `item` - The RSS item.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
async fn process_topics(
    article_text: &str,
    article_url: &str,
    item: &rss::Item,
    params: &mut ProcessItemParams<'_>,
) {
    for topic in params.topics {
        if topic.trim().is_empty() {
            continue;
        }

        let prompt = format!("{} | Determine whether this is specifically about {}. If it is, concisely summarize the information in about 2 paragraphs and then provide a concise one-paragraph analysis of the content and point out any logical fallacies if any. Otherwise just reply with the single word 'No', without any further analysis or explanation.", article_text, topic);
        if let Some(response_text) = generate_llm_response(&prompt, params).await {
            if response_text.trim() != "No" {
                let post_prompt = format!(
                    "Is the article about {}?\n\n{}\n\n{}\n\nRespond with 'Yes' or 'No'.",
                    topic, article_text, response_text
                );
                if let Some(post_response) = generate_llm_response(&post_prompt, params).await {
                    if post_response.trim().starts_with("Yes") {
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
                            .add_article(article_url, true, Some(topic), Some(&response_text))
                            .expect("Failed to add article to database");
                        return;
                    } else {
                        info!("Article is not about '{}': {}", topic, post_response.trim());
                        debug!(" zzz - sleeping 10 seconds ...");
                        sleep(Duration::from_secs(10)).await;
                    }
                }
            } else {
                info!("Article is not about '{}': {}", topic, response_text.trim());
                debug!(" zzz - sleeping 10 seconds ...");
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

/// Handles the case where access to the article is denied, updating the database and logging a warning.
///
/// # Arguments
///
/// * `access_denied` - A boolean indicating whether access was denied.
/// * `article_url` - The URL of the article.
/// * `params` - A mutable reference to `ProcessItemParams` containing the necessary parameters for processing.
async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    params: &mut ProcessItemParams<'_>,
) {
    if access_denied {
        params
            .db
            .add_article(article_url, false, None, None)
            .expect("Failed to add URL to database as access denied");
        warn!(target: TARGET_WEB_REQUEST, "Access denied for URL: {}", article_url);
    }
}

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
///
/// # Arguments
///
/// * `url` - The URL of the article to extract.
///
/// # Returns
///
/// * `Result<String, bool>` - The extracted article text if successful, or a boolean indicating whether access was denied if it fails.
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
