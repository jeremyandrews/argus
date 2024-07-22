use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::options::GenerationOptions;
use std::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::worker::ProcessItemParams;
use crate::TARGET_LLM_REQUEST;

pub async fn generate_llm_response(prompt: &str, params: &ProcessItemParams<'_>) -> Option<String> {
    let max_retries = 3;
    let mut response_text = String::new();
    let mut backoff = 2;

    for retry_count in 0..max_retries {
        let mut request = GenerationRequest::new(params.model.to_string(), prompt.to_string());
        request.options = Some(GenerationOptions::default().temperature(params.temperature));

        info!(target: TARGET_LLM_REQUEST, "Sending LLM request with prompt: {}", prompt);
        match timeout(Duration::from_secs(120), params.ollama.generate(request)).await {
            Ok(Ok(response)) => {
                response_text = response.response;
                info!(target: TARGET_LLM_REQUEST, "LLM response: {}", response_text);
                break;
            }
            Ok(Err(e)) => {
                warn!(target: TARGET_LLM_REQUEST, "Error generating response: {}", e);
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_LLM_REQUEST, "Failed to generate response after {} retries", max_retries);
                }
            }
            Err(_) => {
                warn!(target: TARGET_LLM_REQUEST, "Operation timed out");
                if retry_count < max_retries - 1 {
                    info!(target: TARGET_LLM_REQUEST, "Retrying... ({}/{})", retry_count + 1, max_retries);
                } else {
                    error!(target: TARGET_LLM_REQUEST, "Failed to generate response after {} retries", max_retries);
                }
            }
        }

        if retry_count < max_retries - 1 {
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        }
    }

    if response_text.is_empty() {
        None
    } else {
        Some(response_text)
    }
}
