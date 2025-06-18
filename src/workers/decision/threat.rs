use crate::llm::{generate_json_response, generate_text_response};
use crate::prompt;
use crate::workers::{extract_json_llm_params, extract_text_llm_params};
use crate::{JsonSchemaType, WorkerDetail, TARGET_LLM_REQUEST};
use std::collections::BTreeMap;
use tracing::{debug, info};

/// Checks if the article is about any kind of threat at all.
pub async fn check_if_threat_at_all(
    article_text: &str,
    params: &crate::workers::common::ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) -> bool {
    let text_params = extract_text_llm_params(params);

    // Initial threat check
    let threat_prompt = prompt::threat_prompt(article_text);
    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: asking LLM if about something affecting life or safety.", worker_detail.name, worker_detail.id, worker_detail.model);

    if let Some(response) =
        generate_text_response(&threat_prompt, &text_params, worker_detail).await
    {
        if response.trim().to_lowercase().starts_with("yes") {
            // Confirmation check
            let confirm_prompt = prompt::confirm_threat_prompt(article_text);
            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: confirming if genuine threat to life or safety.", worker_detail.name, worker_detail.id, worker_detail.model);

            if let Some(confirm_response) =
                generate_text_response(&confirm_prompt, &text_params, worker_detail).await
            {
                return confirm_response.trim().to_lowercase().starts_with("yes");
            }
        }
    }
    false
}

/// Processes the places mentioned in the article text and determines affected locations.
/// Returns JSON string of impacted regions if threat detected, empty string otherwise.
pub async fn determine_threat_location(
    article_text: &str,
    places: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    params: &crate::workers::common::ProcessItemParams<'_>,
    worker_detail: &WorkerDetail,
) -> String {
    // Generate the prompt based on the article text and places hierarchy
    let threat_locations_prompt = prompt::threat_locations(article_text, &places);
    debug!(
        target: TARGET_LLM_REQUEST,
        "[{} {} {}]: asking LLM where threat is.",
        worker_detail.name,
        worker_detail.id,
        worker_detail.model
    );

    // Create JSON params for threat location detection
    let json_params = extract_json_llm_params(params, JsonSchemaType::ThreatLocation);

    if let Some(response) =
        generate_json_response(&threat_locations_prompt, &json_params, worker_detail).await
    {
        info!("initial response: {}", response);
        let trimmed_response = response.trim();
        info!("trimmed_response: {}", trimmed_response);

        match serde_json::from_str::<crate::llm::ThreatLocationResponse>(trimmed_response) {
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
                info!("Failed to parse JSON response: {}", e);
            }
        }
    }

    // If no region is impacted, return an empty string
    "".to_string()
}

/// Checks if the article is relevant to the given topic
pub async fn article_is_relevant(
    article_text: &str,
    topic_prompt: &str,
    pub_date: Option<&str>,
    params: &crate::workers::common::ProcessItemParams<'_>,
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

    // Create text params for text generation
    let text_params = extract_text_llm_params(params);

    // Generate summary
    let summary_prompt = prompt::summary_prompt(article_text, pub_date);
    let summary_response = generate_text_response(&summary_prompt, &text_params, worker_detail)
        .await
        .unwrap_or_default();

    // Confirm the article relevance
    let confirm_prompt = prompt::confirm_prompt(&summary_response, topic_prompt);
    if let Some(confirm_response) =
        generate_text_response(&confirm_prompt, &text_params, worker_detail).await
    {
        if confirm_response.trim().to_lowercase().starts_with("yes") {
            return true;
        }
    }
    return false;
}
