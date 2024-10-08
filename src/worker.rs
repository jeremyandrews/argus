use ollama_rs::Ollama;
use rand::{rngs::StdRng, Rng, SeedableRng};
use readability::extractor;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::db::Database;
use crate::llm::generate_llm_response;
use crate::slack::send_to_slack;
use crate::util::weighted_sleep;
use crate::{TARGET_DB, TARGET_LLM_REQUEST, TARGET_WEB_REQUEST};

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

#[derive(Default)]
pub struct FeedItem {
    pub url: String,
    pub title: Option<String>,
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
    let mut rng = StdRng::from_entropy();
    let worker_id = format!("{:?}", std::thread::current().id());

    info!(target: TARGET_LLM_REQUEST, "worker {}: Starting worker loop.", worker_id);

    loop {
        let roll = rng.gen_range(0..100);
        let order = if roll < 30 {
            "newest"
        } else if roll < 55 {
            "oldest"
        } else {
            "random"
        };

        if let Some((url, title)) = db.fetch_and_delete_url_from_queue(order).await.unwrap() {
            if url.trim().is_empty() {
                error!(target: TARGET_LLM_REQUEST, "worker {}: Found an empty URL in the queue, skipping...", worker_id);
                continue;
            }

            info!(target: TARGET_LLM_REQUEST, "worker {}: Moving on to a new URL: {} ({:?})", worker_id, url, title);

            let item = FeedItem {
                url: url.clone(),
                title,
            };

            let mut params = ProcessItemParams {
                topics,
                ollama,
                model,
                temperature,
                db,
                slack_token,
                slack_channel,
                places: places.clone(),
                non_affected_people,
                non_affected_places,
            };

            process_item(item, &mut params).await;
        } else {
            debug!(target: TARGET_LLM_REQUEST, "worker {}: No URLs to process. Sleeping for 1 minute before retrying.", worker_id);
            sleep(Duration::from_secs(60)).await;
            continue;
        }
    }
}

pub async fn process_item(item: FeedItem, params: &mut ProcessItemParams<'_>) {
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(
        target: TARGET_LLM_REQUEST,
        "worker {}: Reviewing => {} ({})",
        worker_id,
        item.title.clone().unwrap_or_default(),
        item.url
    );
    let article_url = item.url;
    let article_title = item.title.unwrap_or_default();

    match extract_article_text(&article_url).await {
        Ok(article_text) => {
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
                    debug!(target: TARGET_LLM_REQUEST, "worker {}: Article is not about an ongoing or imminent threat.", worker_id);
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
                    params,
                )
                .await;
            } else {
                process_topics(
                    &article_text,
                    &article_url,
                    &article_title,
                    &article_hash,
                    params,
                )
                .await;
            }
            weighted_sleep().await;
        }
        Err(access_denied) => {
            handle_access_denied(access_denied, &article_url, &article_title, params).await;
        }
    }
}

/// Checks if the article is about any kind of threat at all.
async fn check_if_threat_at_all(article_text: &str, params: &mut ProcessItemParams<'_>) -> bool {
    let threat_prompt = format!(
        "{} | Is this article about any ongoing or imminent and potentially life-threatening event or situation? Answer yes or no.",
        article_text
    );
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "worker {}: Asking LLM: is this article about an ongoing or imminent and potentially life-threatening event", worker_id);

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
    let worker_id = format!("{:?}", std::thread::current().id());
    let mut article_about_any_area = false;
    for (continent, countries) in places.as_object().unwrap() {
        if !process_continent(&mut place_params, continent, countries, params).await {
            article_about_any_area = true;
            debug!(
                target: TARGET_LLM_REQUEST,
                "worker {}: Article is not about something affecting life or safety on '{}'",
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

    let continent_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living on the continent of {}? Answer yes or no.",
        article_text, continent
    );
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "worker {}: Asking LLM: is this article about ongoing or imminent threat on {}", worker_id, continent);

    let continent_response = match generate_llm_response(&continent_prompt, params).await {
        Some(response) => response,
        None => return false,
    };

    if !continent_response.trim().to_lowercase().starts_with("yes") {
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "worker {}: Article is about something affecting life or safety on '{}'",
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
    let country_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in {} on the continent of {}? Answer yes or no.",
        article_text, country, continent
    );
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "worker {}: Asking LLM: is this article about ongoing or imminent threat in {} on {}", worker_id, country, continent);

    let country_response = match generate_llm_response(&country_prompt, params).await {
        Some(response) => response,
        None => return false,
    };

    if !country_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "worker {}: Article is not about something affecting life or safety in '{}' on '{}'",
            worker_id,
            country,
            continent
        );
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "worker {}: Article is about something affecting life or safety in '{}' on '{}'",
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

    let region_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in the region of {} in the country of {} on {}? Answer yes or no.",
        article_text, region, country, continent
    );
    let worker_id = format!("{:?}", std::thread::current().id());
    debug!(target: TARGET_LLM_REQUEST, "worker {}: Asking LLM: is this article about ongoing or imminent threat in {} in {} on {}", worker_id, region, country, continent);

    let region_response = match generate_llm_response(&region_prompt, proc_params).await {
        Some(response) => response,
        None => return false,
    };

    if !region_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "worker {}: Article is not about something affecting life or safety in '{}', '{}'",
            worker_id,
            region,
            country
        );
        return false;
    }

    debug!(
        target: TARGET_LLM_REQUEST,
        "worker {}: Article is about something affecting life or safety in '{}', '{}'",
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

    let city_prompt = format!(
        "{} | Is this article about an ongoing or imminent and potentially life-threatening event or situation that directly affects the physical safety of people living in or near the city of {} in the region of {} in the country of {} on {}? Answer yes or no.",
        article_text, city_name, region, country, continent
    );

    let worker_id = format!("{:?}", std::thread::current().id());
    let city_response = match generate_llm_response(&city_prompt, proc_params).await {
        Some(response) => response,
        None => return false,
    };

    if !city_response.trim().to_lowercase().starts_with("yes") {
        debug!(
            target: TARGET_LLM_REQUEST,
            "worker {}: Article is not about something affecting life or safety in '{}, {}, {}'",
            worker_id,
            city_name,
            region,
            country
        );
        return false;
    }

    info!(
        target: TARGET_LLM_REQUEST,
        "worker {}: Article is about something affecting life or safety in '{}, {}, {}'",
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
    params: &mut ProcessItemParams<'_>,
) {
    let worker_id = format!("{:?}", std::thread::current().id());
    let formatted_article = format!("*<{}|{}>*", article_url, article_title);

    let mut relation_to_topic_response = String::new();

    // Generate summary
    let summary_prompt = format!(
        "{} | Carefully read and thoroughly understand the provided text. Create a comprehensive summary (without telling me that's what you're doing) in bullet points in American English that cover all the main ideas and key points from the entire text, maintains the original text's structure and flow, and uses clear and concise language. For really short texts (up to 25 words): simply quote the text, for short texts (up to 100 words): 2-4 bullet points, for medium-length texts (501-1000 words): 3-5 bullet points, for long texts (1001-2000 words): 4-8 bullet points, and for very long texts (over 2000 words): 6-10 bullet points. Format for easy and clear readability in ASCII text.",
        article_text
    );
    let summary_response = generate_llm_response(&summary_prompt, params)
        .await
        .unwrap_or_default();

    // Generate tiny summary
    let tiny_summary_prompt = format!(
        "{} | Please summarize down to 200 characters or less. Do not tell me what you're doing.",
        summary_response
    );
    let tiny_summary_response = generate_llm_response(&tiny_summary_prompt, params)
        .await
        .unwrap_or_default();

    // Generate critical analysis
    let critical_analysis_prompt = format!(
        "{} | Carefully read and thoroughly understand the provided text. Please provide a credability score from 1 to 10, where 1 represents highly biased or fallacious content, and 10 represents unbiased, logically sound content. Then on the next line provide a style score from 1 to 10, where 1 represents very poorly written text, and 10 represents eloquent and understandable text. Then on the next line provide a political score that is either Left, Center Left, Center, Center Right, Right, or not-applicable.  Finally on the next line, provide a concise two to three sentence critical analysis of the text in American English. Format for easy and clear reasability in ASCII text.",
        article_text
    );
    let critical_analysis_response = generate_llm_response(&critical_analysis_prompt, params)
        .await
        .unwrap_or_default();

    // Generate logical fallacies
    let logical_fallacies_prompt = format!(
        "{} | Carefully read and throroughly understand the provided text. If there are biases (e.g., confirmation bias, selection bias), logical fallacies (e.g., ad hominem, straw man, false dichotomy) please explain in one or two short sentences. Finally, in one or a maximum of two short sentences identify the strength of arguments and evidence presented. Do all in American English, and without explaining what you are doing. Format for easy and clear reasability in ASCII text.",
        article_text
    );
    let logical_fallacies_response = generate_llm_response(&logical_fallacies_prompt, params)
        .await
        .unwrap_or_default();

    // Generate relation to topic (affected and non-affected summary)
    let mut affected_summary = String::default();
    if !affected_people.is_empty() {
        affected_summary = format!(
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
            "{} | How does this article affect the life and safety of people living in the following places: {}? Answer in a few sentences in American English without explaining what you're doing.",
            article_text,
            affected_places.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        let how_response = generate_llm_response(&how_prompt, params)
            .await
            .unwrap_or_default();
        relation_to_topic_response
            .push_str(&format!("\n\n{}\n\n{}", affected_summary, how_response));
    }

    let mut non_affected_summary = String::default();
    if !non_affected_people.is_empty() {
        non_affected_summary = format!(
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
            "topic": format!("{} {}", affected_summary, non_affected_summary),
            "summary": summary_response,
            "tiny_summary": tiny_summary_response,
            "critical_analysis": critical_analysis_response,
            "logical_fallacies": logical_fallacies_response,
            "relation_to_topic": relation_to_topic_response,
            "model": params.model
        });

        send_to_slack(
            &formatted_article,
            &detailed_response_json.to_string(),
            params.slack_token,
            params.slack_channel,
        )
        .await;

        match params
            .db
            .add_article(
                article_url,
                true,
                None,
                Some(&detailed_response_json.to_string()),
                Some(&article_hash),
            )
            .await
        {
            Ok(_) => {
                debug!(target: TARGET_DB, "worker {}: Successfully added article to database", worker_id)
            }
            Err(e) => {
                error!(target: TARGET_DB, "worker {}: Failed to add article to database: {:?}", worker_id, e)
            }
        }
    }
}

/// Processes the topics mentioned in the article text and sends the results to the Slack channel if relevant.
async fn process_topics(
    article_text: &str,
    article_url: &str,
    article_title: &str,
    article_hash: &str,
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
        debug!(target: TARGET_LLM_REQUEST, "worker {}: Asking LLM: is this article specifically about {}", worker_id, topic_name);

        let yes_no_prompt = format!(
            "{} | Is this article specifically about {}? Answer yes or no.",
            article_text, topic_name
        );

        if let Some(yes_no_response) = generate_llm_response(&yes_no_prompt, params).await {
            if yes_no_response.trim().to_lowercase().starts_with("yes") {
                article_relevant = true;

                let summary_prompt = format!(
                    "{} | Carefully read and thoroughly understand the provided text. Create a comprehensive summary in bullet points in American English that cover all the main ideas and key points from the entire text, maintains the original text's structure and flow, and uses clear and concise language. For really short texts (up to 25 words): simply quote the text, for short texts (up to 100 words): 2-4 bullet points, for medium-length texts (501-1000 words): 3-5 bullet points, for long texts (1001-2000 words): 4-8 bullet points, and for very long texts (over 2000 words): 6-10 bullet points. Please do this without explaining what you're doing. Format for easy and clear readability in ASCII text.",
                    article_text
                );

                let critical_analysis_prompt = format!(
                    "{} | Carefully read and thoroughly understand the provided text. Please provide a credability score from 1 to 10, where 1 represents highly biased or fallacious content, and 10 represents unbiased, logically sound content. Then on the next line provide a style score from 1 to 10, where 1 represents very poorly written text, and 10 represents eloquent and understandable text. Then on the next line provide a political score that is either Left, Center Left, Center, Center Right, Right, or not-applicable.  Finally on the next line, provide a concise two to three sentence critical analysis of the text in American English. Format for easy and clear readability in ASCII text.",
                    article_text
                );

                let logical_fallacies_prompt = format!(
                    "{} | Carefully read and throroughly understand the provided text. If there are biases (e.g., confirmation bias, selection bias), logical fallacies (e.g., ad hominem, straw man, false dichotomy) please explain in one or two short sentences. Finally, in one or a maximum of two short sentences identify the strength of arguments and evidence presented. Do all in American English, and without explaining what you are doing. Format for easy and clear readability in ASCII text.",
                    article_text);

                let relation_prompt = format!(
                    "{} | Briefly explain in American English in one or two short sentences how this relates to {} starting with the words 'This relates to {}`. Do so in American English, without explaining what you're doing. Format for easy and clear readability in ASCII text.",
                    article_text, topic_name, topic_name
                );

                let summary_response = generate_llm_response(&summary_prompt, params)
                    .await
                    .unwrap_or_default();

                // Generate tiny summary
                let tiny_summary_prompt = format!(
                    "{} | Please summarize down to 200 characters or less. Do not tell me what you're doing.",
                    summary_response
                );
                let tiny_summary_response = generate_llm_response(&tiny_summary_prompt, params)
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

                let confirm_prompt = format!(
                    "{} | Is this article really about {} and not a promotion or advertisement? Answer yes or no.",
                    summary_response, topic_name
                );

                if let Some(confirm_response) = generate_llm_response(&confirm_prompt, params).await
                {
                    if confirm_response.trim().to_lowercase().starts_with("yes") {
                        let formatted_article = format!("*<{}|{}>*", article_url, article_title);

                        let detailed_response_json = json!({
                            "topic": topic_name,
                            "summary": summary_response,
                            "tiny_summary": tiny_summary_response,
                            "critical_analysis": critical_analysis_response,
                            "logical_fallacies": logical_fallacies_response,
                            "relation_to_topic": relation_response,
                            "model": params.model
                        });

                        send_to_slack(
                            &formatted_article,
                            &detailed_response_json.to_string(),
                            params.slack_token,
                            params.slack_channel,
                        )
                        .await;

                        match params
                            .db
                            .add_article(
                                article_url,
                                true,
                                Some(topic_name),
                                Some(&detailed_response_json.to_string()),
                                Some(&article_hash),
                            )
                            .await
                        {
                            Ok(_) => {
                                debug!(target: TARGET_DB, "worker {}: Successfully added article about '{}' to database", worker_id, topic_name)
                            }
                            Err(e) => {
                                error!(target: TARGET_DB, "worker {}: Failed to add article about '{}' to database: {:?}", worker_id, topic_name, e)
                            }
                        }

                        return; // No need to continue checking other topics
                    } else {
                        debug!(
                            target: TARGET_LLM_REQUEST,
                            "worker {}: Article is not about '{}' or is a promotion/advertisement: {}",
                            worker_id,
                            topic_name,
                            confirm_response.trim()
                        );
                        weighted_sleep().await;
                    }
                }
            } else {
                debug!(
                    target: TARGET_LLM_REQUEST,
                    "worker {}: Article is not about '{}': {}",
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
            .add_article(article_url, false, None, None, Some(&article_hash))
            .await
        {
            Ok(_) => {
                debug!(target: TARGET_DB, "worker {}: Successfully added non-relevant article to database", worker_id)
            }
            Err(e) => {
                error!(target: TARGET_DB, "worker {}: Failed to add non-relevant article to database: {:?}", worker_id, e)
            }
        }
    }
}

/// Extracts the text of the article from the given URL, retrying up to a maximum number of retries if necessary.
async fn extract_article_text(url: &str) -> Result<String, bool> {
    let max_retries = 3;
    let article_text: String;
    let mut backoff = 2;
    let worker_id = format!("{:?}", std::thread::current().id());

    for retry_count in 0..max_retries {
        let scrape_future = async { extractor::scrape(url) };
        debug!(target: TARGET_WEB_REQUEST, "worker {}: Requesting extraction for URL: {}", worker_id, url);
        match timeout(Duration::from_secs(60), scrape_future).await {
            Ok(Ok(product)) => {
                if product.text.is_empty() {
                    warn!(target: TARGET_WEB_REQUEST, "worker {}: Extracted article is empty for URL: {}", worker_id, url);
                    break;
                }
                article_text = format!("Title: {}\nBody: {}\n", product.title, product.text);
                debug!(target: TARGET_WEB_REQUEST, "worker {}: Extraction succeeded for URL: {}", worker_id, url);
                return Ok(article_text);
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_WEB_REQUEST, "worker {}: Error extracting page: {:?}", worker_id, e);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "worker {}: Retrying... ({}/{})", worker_id, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "worker {}: Failed to extract article after {} retries", worker_id, max_retries);
                }
                if e.to_string().contains("Access Denied") || e.to_string().contains("Unexpected") {
                    return Err(true);
                }
            }
            Err(_) => {
                warn!(target: TARGET_WEB_REQUEST, "worker {}: Operation timed out", worker_id);
                if retry_count < max_retries - 1 {
                    debug!(target: TARGET_WEB_REQUEST, "worker {}: Retrying... ({}/{})", worker_id, retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_WEB_REQUEST, "worker {}: Failed to extract article after {} retries", worker_id, max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2;
        }
    }

    warn!(target: TARGET_WEB_REQUEST, "worker {}: Article text extraction failed for URL: {}", worker_id, url);
    Err(false)
}

/// Handles the case where access to the article is denied, updating the database and logging a warning.
async fn handle_access_denied(
    access_denied: bool,
    article_url: &str,
    article_title: &str,
    params: &mut ProcessItemParams<'_>,
) {
    let worker_id = format!("{:?}", std::thread::current().id());
    if access_denied {
        match params
            .db
            .add_article(article_url, false, None, None, None)
            .await
        {
            Ok(_) => {
                warn!(target: TARGET_WEB_REQUEST, "worker {}: Access denied for URL: {} ({})", worker_id, article_url, article_title)
            }
            Err(e) => {
                error!(target: TARGET_WEB_REQUEST, "worker {}: Failed to add access denied URL '{}' ({}) to database: {:?}", worker_id, article_url, article_title, e)
            }
        }
    }
}
