use async_openai::types::CreateCompletionRequestArgs;
use ollama_rs::generation::{
    completion::request::GenerationRequest,
    options::GenerationOptions,
    parameters::{FormatType, JsonStructure},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};
use unicode_segmentation::UnicodeSegmentation;

use crate::TARGET_LLM_REQUEST;
use crate::{LLMClient, LLMParams, WorkerDetail};

const CONTEXT_WINDOW: u32 = 6144;

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
        "[{} {} {}]: processing LLM prompt: {}.",
        worker_detail.name, worker_detail.id, worker_detail.model, prompt
    );

    // Estimate token count
    let estimated_tokens = estimate_token_count(prompt);

    if estimated_tokens <= CONTEXT_WINDOW {
        info!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {}]: Estimated token count ({}) should fit within context window ({}).",
            worker_detail.name, worker_detail.id, worker_detail.model, estimated_tokens, CONTEXT_WINDOW
        );
    } else {
        warn!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {}]: Estimated token count ({}) may exceed context window ({}). Response may be incomplete.",
            worker_detail.name, worker_detail.id, worker_detail.model, estimated_tokens, CONTEXT_WINDOW
        );
    }

    for retry_count in 0..max_retries {
        match &params.llm_client {
            LLMClient::Ollama(ref ollama) => {
                let mut request = GenerationRequest::new(params.model.clone(), prompt.to_string());

                if params.require_json.unwrap_or(false) {
                    request.format = Some(FormatType::StructuredJson(JsonStructure::new::<
                        ThreatLocationResponse,
                    >()));
                }

                let options = GenerationOptions::default()
                    .temperature(params.temperature)
                    .num_ctx(CONTEXT_WINDOW);
                request.options = Some(options);

                debug!(
                    target: TARGET_LLM_REQUEST,
                    "[{} {} {}]: Ollama processing LLM prompt: {}.",
                    worker_detail.name, worker_detail.id, worker_detail.model, prompt
                );

                match timeout(Duration::from_secs(120), ollama.generate(request)).await {
                    Ok(Ok(response)) => {
                        response_text = response.response;
                        debug!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: Ollama response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, response_text
                        );
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: error generating Ollama response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, e
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: Ollama request timed out.",
                            worker_detail.name, worker_detail.id, worker_detail.model
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
                    "[{} {} {}]: OpenAI processing LLM prompt: {}.",
                    worker_detail.name, worker_detail.id, worker_detail.model, prompt
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
                            debug!(
                                target: TARGET_LLM_REQUEST,
                                "[{} {} {}]: OpenAI response: {}.",
                                worker_detail.name, worker_detail.id, worker_detail.model, response_text
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: error generating OpenAI response: {}.",
                            worker_detail.name, worker_detail.id, worker_detail.model, e
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            "[{} {} {}]: OpenAI request timed out.",
                            worker_detail.name, worker_detail.id, worker_detail.model
                        );
                    }
                }
            }
        }

        if retry_count < max_retries - 1 {
            info!(
                target: TARGET_LLM_REQUEST,
                "[{} {} {}]: sleeping {} seconds.",
                worker_detail.name, worker_detail.id, worker_detail.model, backoff
            );
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                target: TARGET_LLM_REQUEST,
                "[{} {} {}]: failed to generate response after {} retries.",
                worker_detail.name, worker_detail.id, worker_detail.model, max_retries
            );
        }
    }

    if response_text.is_empty() {
        error!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {}]: no response after all retries.",
            worker_detail.name, worker_detail.id, worker_detail.model
        );
        None
    } else {
        debug!(
            target: TARGET_LLM_REQUEST,
            "[{} {} {}]: successfully generated response.",
            worker_detail.name, worker_detail.id, worker_detail.model
        );
        Some(response_text)
    }
}
