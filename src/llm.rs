use async_openai::types::CreateCompletionRequestArgs;
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::options::GenerationOptions;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use crate::worker::ProcessItemParams;
use crate::LLMClient;
use crate::TARGET_LLM_REQUEST;

pub async fn generate_llm_response(prompt: &str, params: &ProcessItemParams<'_>) -> Option<String> {
    let max_retries = 5;
    let mut response_text = String::new();
    let mut backoff = 2;
    let worker_id = format!("{:?}", std::thread::current().id()); // Retrieve the worker number

    debug!(target: TARGET_LLM_REQUEST, "Worker {}: Starting LLM response generation for prompt: {}", worker_id, prompt);

    for retry_count in 0..max_retries {
        match params.llm_client {
            LLMClient::Ollama(ref ollama) => {
                let mut request =
                    GenerationRequest::new(params.model.to_string(), prompt.to_string());
                request.options =
                    Some(GenerationOptions::default().temperature(params.temperature));

                debug!(target: TARGET_LLM_REQUEST, "Worker {}: Sending Ollama LLM request with prompt: {}", worker_id, prompt);

                match timeout(Duration::from_secs(120), ollama.generate(request)).await {
                    Ok(Ok(response)) => {
                        response_text = response.response;
                        debug!(target: TARGET_LLM_REQUEST, "Worker {}: Ollama LLM response received: {}", worker_id, response_text);
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!(target: TARGET_LLM_REQUEST, "Worker {}: Error generating Ollama response: {}", worker_id, e);
                    }
                    Err(_) => {
                        warn!(target: TARGET_LLM_REQUEST, "Worker {}: Ollama LLM request timed out", worker_id);
                    }
                }
            }
            LLMClient::OpenAI(ref openai_client) => {
                let request = CreateCompletionRequestArgs::default()
                    .model(params.model)
                    .prompt(prompt)
                    .temperature(params.temperature)
                    .build()
                    .expect("Failed to build OpenAI request");

                debug!(target: TARGET_LLM_REQUEST, "Worker {}: Sending OpenAI LLM request with prompt: {}", worker_id, prompt);

                match timeout(
                    Duration::from_secs(120),
                    openai_client.completions().create(request),
                )
                .await
                {
                    Ok(Ok(response)) => {
                        if let Some(choice) = response.choices.first() {
                            response_text = choice.text.clone();
                            debug!(target: TARGET_LLM_REQUEST, "Worker {}: OpenAI LLM response received: {}", worker_id, response_text);
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(target: TARGET_LLM_REQUEST, "Worker {}: Error generating OpenAI response: {}", worker_id, e);
                    }
                    Err(_) => {
                        warn!(target: TARGET_LLM_REQUEST, "Worker {}: OpenAI LLM request timed out", worker_id);
                    }
                }
            }
        }

        if retry_count < max_retries - 1 {
            info!(target: TARGET_LLM_REQUEST, "Worker {}: Backing off for {} seconds before retry", worker_id, backoff);
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(target: TARGET_LLM_REQUEST, "Worker {}: Failed to generate response after {} retries", worker_id, max_retries);
        }
    }

    if response_text.is_empty() {
        error!(target: TARGET_LLM_REQUEST, "Worker {}: No response generated after all retries", worker_id);
        None
    } else {
        debug!(target: TARGET_LLM_REQUEST, "Worker {}: Successfully generated response", worker_id);
        Some(response_text)
    }
}
