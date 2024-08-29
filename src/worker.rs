use ollama_rs::Ollama;
use readability::extractor;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::slack::send_to_slack;
use crate::util::weighted_sleep;
use crate::TARGET_WEB_REQUEST;

/// Parameters required for processing an item, including topics, database, and Slack channel information.
pub struct ProcessItemParams<'a> {
    pub topics: &'a [String],
    pub ollama: &'a Ollama,
    pub model: &'a str,
    pub temperature: f32,
    pub db: &'a Database,
    pub slack_token: &'a str,
    pub slack_channel: &'a str,
    pub places: Option<Value>,
    pub non_affected_people: &'a mut BTreeSet<String>,
    pub non_affected_places: &'a mut BTreeSet<String>,
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

pub async fn worker_loop(
    topics: &[String],
    ollama: &Ollama,
    model: &str,
    temperature: f32,
    slack_token: &str,
    slack_channel: &str,
    places: Option<Value>,
    non_affected_people: &mut BTreeSet<String>,
    non_affected_places: &mut BTreeSet<String>,
) {
    let db = Database::instance().await;
    loop {
        if let Some(url) = db.fetch_and_delete_url_from_queue().await.unwrap() {
            // Validate the URL
            if let Ok(parsed_url) = Url::parse(&url) {
                info!("Processing URL: {}", parsed_url);
                let item = rss::Item {
                    link: Some(url.clone()),
                    ..Default::default() // Add other fields if necessary
                };

                let mut params = ProcessItemParams {
                    topics,
                    ollama,
                    model,
                    temperature,
                    db: &db,
                    slack_token,
                    slack_channel,
                    places: places.clone(),
                    non_affected_people,
                    non_affected_places,
                };

                process_item(item, &mut params).await;
            } else {
                error!("Invalid URL found in queue: {}", url);
                // Optionally, handle invalid URLs (e.g., remove them from the queue)
            }
        } else {
            // No more URLs to process, break the loop
            break;
        }
    }
}

/// Processes a single RSS item by extracting and analyzing the article text, and updating the affected people and places lists.
pub async fn process_item(item: rss::Item, params: &mut ProcessItemParams<'_>) {
    info!(
        " - reviewing => {} ({})",
        item.title.clone().unwrap_or_default(),
        item.link.clone().unwrap_or_default()
    );
    let article_url = item.link.clone().unwrap_or_default();

    match extract_article_text(&article_url).await {
        Ok(article_text) => {
            let mut affected_regions = BTreeSet::new();
            let mut affected_people = BTreeSet::new();
            let mut affected_places = BTreeSet::new();
            let mut non_affected_people = BTreeSet::new();
            let mut non_affected_places = BTreeSet::new();

            // Extract places from params
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

                // Preliminary threat check
                if check_if_threat_at_all(&article_text, params).await {
                    process_places(place_params, &places, params).await;
                } else {
                    debug!("Article is not about an ongoing or imminent threat at all.");
                }
            }

            if !affected_people.is_empty() || !non_affected_people.is_empty() {
                summarize_and_send_article(
                    &article_url,
                    &item,
                    &article_text,
                    &affected_regions,
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
            weighted_sleep().await;
        }
        Err(access_denied) => handle_access_denied(access_denied, &article_url, params).await,
    }
}

/// Checks if the article is about any kind of threat at all.
async fn check_if_threat_at_all(article_text: &str, params: &mut ProcessItemParams<'_>) -> bool {
    let threat_prompt = format!(
        "{} | Is this article about any ongoing or imminent and potentially life-threatening event or situation? Answer yes or no.",
        article_text
    );
    info!(target: TARGET_WEB_REQUEST, "Asking LLM: is this article about an ongoing or immenent and potentially life-threatening event");

    match generate_llm_response(&threat_prompt, params).await {
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
    for (continent, countries) in places.as_object().unwrap() {
        if !process_continent(&mut place_params, continent, countries, params).await {
            debug!(
                "Article is not about something affecting life or safety on '{}'",
                continent
            );
        }
        weighted_sleep().await;
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

    let continent_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living on the continent of {}? Answer yes or no.",
        article_text, continent
    );
    info!(target: TARGET_WEB_REQUEST, "Asking LLM: is this article about ongoing or imminent threat on {}", continent);

    let continent_response = match generate_llm_response(&continent_prompt, params).await {
        Some(response) => response,
        None => return false,
    };

    if !continent_response.trim().to_lowercase().starts_with("yes") {
        return false;
    }

    info!(
        "Article is about something affecting life or safety on '{}'",
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
    let country_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in {} on the continent of {}? Answer yes or no.",
        article_text, country, continent
    );
    info!(target: TARGET_WEB_REQUEST, "Asking LLM: is this article about ongoing or imminent threat in {} on {}", country, continent);

    let country_response = match generate_llm_response(&country_prompt, params).await {
        Some(response) => response,
        None => return false,
    };

    if !country_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            "Article is not about something affecting life or safety in '{}' on '{}'",
            country, continent
        );
        return false;
    }

    info!(
        "Article is about something affecting life or safety in '{}' on '{}'",
        country, continent
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

    let region_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in the region of {} in the country of {} on {}? Answer yes or no.",
        article_text, region, country, continent
    );
    info!(target: TARGET_WEB_REQUEST, "Asking LLM: is this article about ongoing or imminent threat in {} in {} on {}", region, country, continent);

    let region_response = match generate_llm_response(&region_prompt, proc_params).await {
        Some(response) => response,
        None => return false,
    };

    if !region_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            "Article is not about something affecting life or safety in '{}', '{}'",
            region, country
        );
        return false;
    }

    info!(
        "Article is about something affecting life or safety in '{}', '{}'",
        region, country
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

    let city_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in or near the city of {} in the region of {} in the country of {} on {}? Answer yes or no.",
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

    info!(
        "Article is about something affecting life or safety in '{}, {}, {}'",
        city_name, region, country
    );

    affected_people.insert(format!(
        "{} {} ({}) in {}",
        city_data[0], city_data[1], city_data[5], city_name
    ));
    affected_places.insert(format!("{} in {} on {}", city_name, country, continent));

    true
}

/// Summarizes and sends the article to the Slack channel, and updates the database with the article data.
async fn summarize_and_send_article(
    article_url: &str,
    item: &rss::Item,
    article_text: &str,
    affected_regions: &BTreeSet<String>,
    affected_people: &BTreeSet<String>,
    affected_places: &BTreeSet<String>,
    non_affected_people: &BTreeSet<String>,
    non_affected_places: &BTreeSet<String>,
    params: &mut ProcessItemParams<'_>,
) {
    let formatted_article = format!(
        "*<{}|{}>*",
        article_url,
        item.title.clone().unwrap_or_default()
    );

    let mut relation_to_topic_response = String::new();

    // Generate summary
    let summary_prompt = format!(
        "{} | Carefully read and thoroughly understand the provided text. Create a comprehensive summary (without telling me that's what you're doing) in bullet points in American English that cover all the main ideas and key points from the entire text, maintains the original text's structure and flow, and uses clear and concise language. For really short texts (up to 25 words): simply quote the text, for short texts (up to 100 words): 2-4 bullet points, for medium-length texts (501-1000 words): 3-5 bullet points, for long texts (1001-2000 words): 4-8 bullet points, and for very long texts (over 2000 words): 6-10 bullet points",
        article_text
    );
    let summary_response = generate_llm_response(&summary_prompt, params)
        .await
        .unwrap_or_default();

    // Generate critical analysis
    let critical_analysis_prompt = format!(
        "{} | Provide a concise two to three sentence critical analysis in American English (wihtout telling me that's what you're doing) that also includes a credibility score from 1 to 10, where 1 represents highly biased or fallacious content, and 10 represents unbiased, logically sound content.",
        article_text
    );
    let critical_analysis_response = generate_llm_response(&critical_analysis_prompt, params)
        .await
        .unwrap_or_default();

    // Generate logical fallacies
    let logical_fallacies_prompt = format!(
        "{} | Concisely point out in one to three sentences of American English (withot telling me that's what you're doing) if there is any potential biases (e.g., confirmation bias, selection bias), logical fallacies (e.g., ad hominem, straw man, false dichotomy), and identifies strength of arguments and evidence presented",
        article_text
    );
    let logical_fallacies_response = generate_llm_response(&logical_fallacies_prompt, params)
        .await
        .unwrap_or_default();

    // Generate relation to topic (affected and non-affected summary)
    if !affected_people.is_empty() {
        let affected_summary = format!(
            "This article affects these people in {}: {}",
            affected_regions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            affected_people
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        let how_prompt = format!(
            "{} | How does this article affect the life and safety of people living in the following places: {}? Answer in a few sentences in American Engish (without telling me what you're doing).",
            article_text,
            affected_places.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        let how_response = generate_llm_response(&how_prompt, params)
            .await
            .unwrap_or_default();
        relation_to_topic_response
            .push_str(&format!("\n\n{}\n\n{}", affected_summary, how_response));
    }

    if !non_affected_people.is_empty() {
        let non_affected_summary = format!(
            "This article does not affect these people in {}: {}",
            affected_regions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            non_affected_people
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        let why_not_prompt = format!(
            "{} | Why does this article not affect the life and safety of people living in the following places: {}? Answer in a few sentences.",
            article_text,
            non_affected_places.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        let why_not_response = generate_llm_response(&why_not_prompt, params)
            .await
            .unwrap_or_default();
        relation_to_topic_response.push_str(&format!(
            "\n\n{}\n\n{}",
            non_affected_summary, why_not_response
        ));
    }

    if !summary_response.is_empty()
        || !critical_analysis_response.is_empty()
        || !logical_fallacies_response.is_empty()
        || !relation_to_topic_response.is_empty()
    {
        let detailed_response_json = json!({
            "summary": summary_response,
            "critical_analysis": critical_analysis_response,
            "logical_fallacies": logical_fallacies_response,
            "relation_to_topic": relation_to_topic_response
        });

        send_to_slack(
            &formatted_article,
            &detailed_response_json.to_string(),
            params.slack_token,
            params.slack_channel,
        )
        .await;

        // Add detailed logging and error handling around database operations
        match params
            .db
            .add_article(
                article_url,
                true,
                None,
                Some(&detailed_response_json.to_string()),
            )
            .await
        {
            Ok(_) => info!("Successfully added article to database"),
            Err(e) => error!("Failed to add article to database: {:?}", e),
        }
    }
}

/// Processes the topics mentioned in the article text and sends the results to the Slack channel if relevant.
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

        info!(target: TARGET_WEB_REQUEST, "Asking LLM: is this article specifically about {}", topic);

        // First ask a simple yes/no question
        let yes_no_prompt = format!(
            "{} | Is this article specifically about {}? Answer yes or no.",
            article_text, topic
        );

        if let Some(yes_no_response) = generate_llm_response(&yes_no_prompt, params).await {
            if yes_no_response.trim().to_lowercase().starts_with("yes") {
                // Make detailed LLM requests for each section
                let summary_prompt = format!(
                    "{} | Carefully read and thoroughly understand the provided text. Create a comprehensive summary (without telling me that's what you're doing) in bullet points in American English that cover all the main ideas and key points from the entire text, maintains the original text's structure and flow, and uses clear and concise language. For really short texts (up to 25 words): simply quote the text, for short texts (up to 100 words): 2-4 bullet points, for medium-length texts (501-1000 words): 3-5 bullet points, for long texts (1001-2000 words): 4-8 bullet points, and for very long texts (over 2000 words): 6-10 bullet points",
                    article_text
                );

                let critical_analysis_prompt = format!(
                    "{} | Provide a concise two to three sentence critical analysis in American English (wihtout telling me that's what you're doing) that also includes a credibility score from 1 to 10, where 1 represents highly biased or fallacious content, and 10 represents unbiased, logically sound content.",
                    article_text
                );

                let logical_fallacies_prompt = format!(
                    "{} | Concisely point out in one to three sentences of American English (withot telling me that's what you're doing) if there is any potential biases (e.g., confirmation bias, selection bias), logical fallacies (e.g., ad hominem, straw man, false dichotomy), and identifies strength of arguments and evidence presented",
                     article_text);

                let relation_prompt = format!(
                    "{} | Briefly explain in American English (without telling me that's what you're doing) in one to three sentences how this relates to {} starting with the words 'This relates to {}`.",
                    article_text, topic, topic
                );

                let summary_response = generate_llm_response(&summary_prompt, params)
                    .await
                    .unwrap_or_default();
                let critical_analysis_response =
                    generate_llm_response(&critical_analysis_prompt, params)
                        .await
                        .unwrap_or_default();
                let logical_fallacies_response =
                    generate_llm_response(&logical_fallacies_prompt, params)
                        .await
                        .unwrap_or_default();
                let relation_response = generate_llm_response(&relation_prompt, params)
                    .await
                    .unwrap_or_default();

                // Ask again to be sure it's really about the topic and not a promotion or advertisement
                let confirm_prompt = format!(
                    "{} | Is this article really about {} and not a promotion or advertisement? Answer yes or no.",
                    summary_response, topic
                );

                if let Some(confirm_response) = generate_llm_response(&confirm_prompt, params).await
                {
                    if confirm_response.trim().to_lowercase().starts_with("yes") {
                        let formatted_article = format!(
                            "*<{}|{}>*",
                            article_url,
                            item.title.clone().unwrap_or_default()
                        );

                        let detailed_response_json = json!({
                            "summary": summary_response,
                            "critical_analysis": critical_analysis_response,
                            "logical_fallacies": logical_fallacies_response,
                            "relation_to_topic": relation_response
                        });

                        send_to_slack(
                            &formatted_article,
                            &detailed_response_json.to_string(),
                            params.slack_token,
                            params.slack_channel,
                        )
                        .await;

                        params
                            .db
                            .add_article(
                                article_url,
                                true,
                                Some(topic),
                                Some(&detailed_response_json.to_string()),
                            )
                            .await
                            .expect("Failed to add article to database");

                        return;
                    } else {
                        debug!(
                            "Article is not about '{}' or is a promotion/advertisement: {}",
                            topic,
                            confirm_response.trim()
                        );
                        weighted_sleep().await;
                    }
                }
            } else {
                debug!(
                    "Article is not about '{}': {}",
                    topic,
                    yes_no_response.trim()
                );
                weighted_sleep().await;
            }
        }
    }
}

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
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

/// Handles the case where access to the article is denied, updating the database and logging a warning.
async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    params: &mut ProcessItemParams<'_>,
) {
    if access_denied {
        params
            .db
            .add_article(article_url, false, None, None)
            .await
            .expect("Failed to add URL to database as access denied");
        warn!(target: TARGET_WEB_REQUEST, "Access denied for URL: {}", article_url);
    }
}
