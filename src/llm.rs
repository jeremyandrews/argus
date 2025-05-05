use async_openai::types::CreateCompletionRequestArgs;
use ollama_rs::generation::{
    completion::request::GenerationRequest,
    options::GenerationOptions,
    parameters::{FormatType, JsonStructure},
};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use unicode_segmentation::UnicodeSegmentation;

use crate::TARGET_LLM_REQUEST;
use crate::{JsonSchemaType, LLMClient, LLMParams, WorkerDetail};

const CONTEXT_WINDOW: u32 = 8192;

// Response schema for threat location analysis
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ThreatLocationResponse {
    pub impacted_regions: Vec<ImpactedRegion>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct ImpactedRegion {
    pub continent: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
}

// Response schema for entity extraction
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct EntityExtractionResponse {
    pub event_date: Option<String>,
    pub entities: Vec<EntityItem>,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct EntityItem {
    pub name: String,
    pub normalized_name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub importance: String,
    // Additional fields might be present but aren't required by the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Strips <think>...</think> tags from text.
///
/// This function removes all content between <think> and </think> tags,
/// which is used for thinking/reasoning models that output their reasoning
/// inside these tags.
///
/// # Arguments
///
/// * `text` - The text from which to strip thinking tags
///
/// # Returns
///
/// A String with all thinking tags and their content removed
fn strip_thinking_tags(text: &str) -> String {
    // Create a regex pattern to match <think>...</think> blocks
    // Use (?s) to make dot match newlines
    let pattern = r"(?s)<think>.*?</think>";
    let re = Regex::new(pattern).unwrap_or_else(|e| {
        error!("Failed to compile thinking tags regex pattern: {}", e);
        Regex::new(r"nevermatchanything").unwrap()
    });

    // Replace matches with empty string and trim the result
    let result = re.replace_all(text, "").trim().to_string();

    // If the result is empty after stripping, return the original text
    if result.is_empty() {
        return text.to_string();
    }

    result
}

fn estimate_token_count(text: &str) -> u32 {
    // Split into words, considering Unicode graphemes
    let words: Vec<&str> = text.unicode_words().collect();

    // Count of words
    let word_count = words.len();

    // Count of punctuation and special characters
    let punct_count = text.chars().filter(|c| c.is_ascii_punctuation()).count();

    // Rough estimate: assume each word is one token, each punctuation is one token
    // and add some extra tokens for potential subword tokenization
    (word_count + punct_count + (word_count / 2))
        .try_into()
        .unwrap()
}

pub async fn generate_llm_response(
    prompt: &str,
    params: &LLMParams,
    worker_detail: &WorkerDetail,
) -> Option<String> {
    let max_retries = 5;
    let mut response_text = String::new();
    let mut backoff = 2;

    debug!(
        target: TARGET_LLM_REQUEST,
        "[{} {} {} {}]: processing LLM prompt: {}.",
        worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, prompt
    );

    // Estimate token count
    let estimated_tokens = estimate_token_count(prompt);

    if estimated_tokens <= CONTEXT_WINDOW {
        debug!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {} {}]: Estimated token count ({}) should fit within context window ({}).",
            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, estimated_tokens, CONTEXT_WINDOW
        );
    } else {
        warn!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {} {}]: Estimated token count ({}) may exceed context window ({}). Response may be incomplete.",
            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, estimated_tokens, CONTEXT_WINDOW
        );
    }

    for retry_count in 0..max_retries {
        match &params.llm_client {
            LLMClient::Ollama(ref ollama) => {
                // Determine prompt based on no_think flag
                let actual_prompt = if params.no_think {
                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "[{} {} {} {}]: Using no-think mode with '/no_think' suffix",
                        worker_detail.name, worker_detail.id, worker_detail.model,
                        worker_detail.connection_info
                    );
                    format!("{} /no_think", prompt)
                } else {
                    prompt.to_string()
                };

                let mut request = GenerationRequest::new(params.model.clone(), actual_prompt);

                // Apply JSON formatting if specified
                if let Some(json_type) = &params.json_format {
                    match json_type {
                        JsonSchemaType::EntityExtraction => {
                            // Use simpler Json format for entity extraction
                            request.format = Some(FormatType::Json);
                        }
                        JsonSchemaType::ThreatLocation => {
                            request.format =
                                Some(FormatType::StructuredJson(JsonStructure::new::<
                                    ThreatLocationResponse,
                                >(
                                )));
                        }
                        JsonSchemaType::Generic => {
                            request.format = Some(FormatType::Json);
                        }
                    }
                } else if params.require_json.unwrap_or(false) {
                    // Legacy support for require_json
                    request.format = Some(FormatType::StructuredJson(JsonStructure::new::<
                        ThreatLocationResponse,
                    >()));
                }

                // Apply model configuration based on mode
                if params.no_think {
                    // In no_think mode, use standard parameters
                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "[{} {} {} {}]: Using standard parameters for no-think mode",
                        worker_detail.name, worker_detail.id, worker_detail.model,
                        worker_detail.connection_info
                    );

                    let options = GenerationOptions::default()
                        .temperature(params.temperature)
                        .num_ctx(CONTEXT_WINDOW.into());
                    request.options = Some(options);
                } else if let Some(thinking_config) = &params.thinking_config {
                    // Regular thinking model configuration
                    debug!(
                        target: TARGET_LLM_REQUEST,
                        "[{} {} {} {}]: Configuring thinking model with topP={}, topK={}.",
                        worker_detail.name, worker_detail.id, worker_detail.model,
                        worker_detail.connection_info,
                        thinking_config.top_p, thinking_config.top_k
                    );

                    // Note: min_p is not available in the current version of ollama-rs
                    let options = GenerationOptions::default()
                        .temperature(params.temperature)
                        .top_p(thinking_config.top_p)
                        .top_k(thinking_config.top_k as u32)
                        .num_ctx(CONTEXT_WINDOW.into());
                    request.options = Some(options);
                } else {
                    // Regular non-thinking model configuration
                    let options = GenerationOptions::default()
                        .temperature(params.temperature)
                        .num_ctx(CONTEXT_WINDOW.into());
                    request.options = Some(options);
                }

                // Log detailed request information
                debug!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {} {}]: Ollama processing LLM prompt: {}.",
                    worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, prompt
                );

                debug!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {} {}]: Ollama request details: model={}, format={:?}, options={:?}",
                    worker_detail.name, worker_detail.id, worker_detail.model,
                    worker_detail.connection_info,
                    request.model_name,
                    request.format,
                    request.options
                );

                match timeout(Duration::from_secs(120), ollama.generate(request)).await {
                    Ok(Ok(response)) => {
                        response_text = response.response;

                        // Handle the response based on mode
                        if params.no_think {
                            // For no_think mode, check for unexpected thinking tags
                            if response_text.contains("<think>") {
                                error!(
                                    target: TARGET_LLM_REQUEST,
                                    "[{} {} {} {}]: Response contains thinking tags despite no-think mode being enabled. This indicates an issue with the model configuration.",
                                    worker_detail.name, worker_detail.id, worker_detail.model,
                                    worker_detail.connection_info
                                );
                            }
                        } else if let Some(thinking_config) = &params.thinking_config {
                            // Process thinking tags for normal thinking mode
                            if thinking_config.strip_thinking_tags {
                                debug!(
                                    target: TARGET_LLM_REQUEST,
                                    "[{} {} {} {}]: Response contains thinking tags: {}",
                                    worker_detail.name, worker_detail.id, worker_detail.model,
                                    worker_detail.connection_info,
                                    response_text.contains("<think>")
                                );

                                let original_text = response_text.clone();
                                response_text = strip_thinking_tags(&response_text);

                                if response_text != original_text {
                                    debug!(
                                        target: TARGET_LLM_REQUEST,
                                        "[{} {} {} {}]: Stripped thinking tags from response.",
                                        worker_detail.name, worker_detail.id, worker_detail.model,
                                        worker_detail.connection_info
                                    );
                                } else {
                                    warn!(
                                        target: TARGET_LLM_REQUEST,
                                        "[{} {} {} {}]: Expected thinking tags but none found in response.",
                                        worker_detail.name, worker_detail.id, worker_detail.model,
                                        worker_detail.connection_info
                                    );
                                }

                                if response_text.trim().is_empty() {
                                    error!(
                                        target: TARGET_LLM_REQUEST,
                                        "[{} {} {} {}]: Empty response after stripping thinking tags.",
                                        worker_detail.name, worker_detail.id, worker_detail.model,
                                        worker_detail.connection_info
                                    );
                                    response_text =
                                        "Error: Empty response after stripping thinking tags."
                                            .to_string();
                                }
                            }
                        }

                        debug!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: Ollama response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, response_text
                        );
                        break;
                    }
                    Ok(Err(e)) => {
                        // Log the standard error message
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: error generating Ollama response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, e
                        );

                        // Log more detailed error information in debug format
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: Detailed Ollama error: {:?}",
                            worker_detail.name, worker_detail.id, worker_detail.model,
                            worker_detail.connection_info, e
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: Ollama request timed out.",
                            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info
                        );
                    }
                }
            }
            LLMClient::OpenAI(ref openai_client) => {
                let request = CreateCompletionRequestArgs::default()
                    .model(params.model.clone())
                    .prompt(prompt)
                    .temperature(params.temperature)
                    .build()
                    .expect("Failed to build OpenAI request");

                debug!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {} {}]: OpenAI processing LLM prompt: {}.",
                    worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, prompt
                );

                match timeout(
                    Duration::from_secs(120),
                    openai_client.completions().create(request),
                )
                .await
                {
                    Ok(Ok(response)) => {
                        if let Some(choice) = response.choices.first() {
                            response_text = choice.text.clone();

                            // Process thinking tags if needed
                            if let Some(thinking_config) = &params.thinking_config {
                                if thinking_config.strip_thinking_tags {
                                    debug!(
                                        target: TARGET_LLM_REQUEST,
                                        "[{} {} {} {}]: Checking OpenAI response for thinking tags: {}",
                                        worker_detail.name, worker_detail.id, worker_detail.model,
                                        worker_detail.connection_info,
                                        response_text.contains("<think>")
                                    );

                                    let original_text = response_text.clone();
                                    response_text = strip_thinking_tags(&response_text);

                                    if response_text != original_text {
                                        debug!(
                                            target: TARGET_LLM_REQUEST,
                                            "[{} {} {} {}]: Stripped thinking tags from OpenAI response.",
                                            worker_detail.name, worker_detail.id, worker_detail.model,
                                            worker_detail.connection_info
                                        );
                                    } else {
                                        warn!(
                                            target: TARGET_LLM_REQUEST,
                                            "[{} {} {} {}]: Expected thinking tags but none found in OpenAI response.",
                                            worker_detail.name, worker_detail.id, worker_detail.model,
                                            worker_detail.connection_info
                                        );
                                    }

                                    if response_text.trim().is_empty() {
                                        error!(
                                            target: TARGET_LLM_REQUEST,
                                            "[{} {} {} {}]: Empty OpenAI response after stripping thinking tags.",
                                            worker_detail.name, worker_detail.id, worker_detail.model,
                                            worker_detail.connection_info
                                        );
                                        response_text =
                                            "Error: Empty response after stripping thinking tags."
                                                .to_string();
                                    }
                                }
                            }

                            debug!(
                                target: TARGET_LLM_REQUEST,
                                "[{} {} {} {}]: OpenAI response: {}.",
                                worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, response_text
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: error generating OpenAI response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, e
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {} {}]: OpenAI request timed out.",
                            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info
                        );
                    }
                }
            }
        }

        if retry_count < max_retries - 1 {
            info!(
                target: TARGET_LLM_REQUEST,
                "[{} {} {} {}]: sleeping {} seconds.",
                worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, backoff
            );
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                target: TARGET_LLM_REQUEST,
                "[{} {} {} {}]: failed to generate response after {} retries.",
                worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info, max_retries
            );
        }
    }

    if response_text.is_empty() {
        error!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {} {}]: no response after all retries.",
            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info
        );
        None
    } else {
        debug!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {} {}]: successfully generated response.",
            worker_detail.name, worker_detail.id, worker_detail.model, worker_detail.connection_info
        );
        Some(response_text)
    }
}
