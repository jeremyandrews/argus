use async_openai::types::CreateCompletionRequestArgs;
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::options::GenerationOptions;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use crate::TARGET_LLM_REQUEST;
use crate::{LLMClient, LLMParams, WorkerDetail};

pub async fn generate_llm_response(
    prompt: &str,
    params: &LLMParams,
    worker_detail: &WorkerDetail,
) -> Option<String> {
    let max_retries = 5;
    let mut response_text = String::new();
    let mut backoff = 2;

    debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: processing LLM prompt: {}.", worker_detail.name, worker_detail.id, worker_detail.model, prompt);

    for retry_count in 0..max_retries {
        match params.llm_client {
            LLMClient::Ollama(ref ollama) => {
                let mut request =
                    GenerationRequest::new(params.model.to_string(), prompt.to_string());
                request.options =
                    Some(GenerationOptions::default().temperature(params.temperature));

                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Ollama processing LLM prompt: {}.", worker_detail.name, worker_detail.id, worker_detail.model, prompt);

                match timeout(Duration::from_secs(120), ollama.generate(request)).await {
                    Ok(Ok(response)) => {
                        response_text = response.response;
                        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Ollama response: {}.", worker_detail.name, worker_detail.id, worker_detail.model, response_text);
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error generating Ollama response: {}.", worker_detail.name, worker_detail.id, worker_detail.model, e);
                    }
                    Err(_) => {
                        warn!(target: TARGET_LLM_REQUEST, "[{} {} {}]: Ollama request timed out.", worker_detail.name, worker_detail.id, worker_detail.model);
                    }
                }
            }
            LLMClient::OpenAI(ref openai_client) => {
                let request = CreateCompletionRequestArgs::default()
                    .model(params.model.to_string())
                    .prompt(prompt)
                    .temperature(params.temperature)
                    .build()
                    .expect("Failed to build OpenAI request");

                debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: OpenAI processing LLM prompt: {}.", worker_detail.name, worker_detail.id, worker_detail.model, prompt);

                match timeout(
                    Duration::from_secs(120),
                    openai_client.completions().create(request),
                )
                .await
                {
                    Ok(Ok(response)) => {
                        if let Some(choice) = response.choices.first() {
                            response_text = choice.text.clone();
                            debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: OpenAI response: {}.", worker_detail.name, worker_detail.id, worker_detail.model, response_text);
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(target: TARGET_LLM_REQUEST, "[{} {} {}]: error generating OpenAI response: {}.", worker_detail.name, worker_detail.id, worker_detail.model, e);
                    }
                    Err(_) => {
                        warn!(target: TARGET_LLM_REQUEST, "[{} {} {}]: OpenAI request timed out.", worker_detail.name, worker_detail.id, worker_detail.model);
                    }
                }
            }
        }

        if retry_count < max_retries - 1 {
            info!(target: TARGET_LLM_REQUEST, "[{} {} {}]: sleeping {} seconds.", worker_detail.name, worker_detail.id, worker_detail.model, backoff);
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: failed to generate response after {} retries.", worker_detail.name, worker_detail.id, worker_detail.model, max_retries);
        }
    }

    if response_text.is_empty() {
        error!(target: TARGET_LLM_REQUEST, "[{} {} {}]: no response after all retries.", worker_detail.name, worker_detail.id, worker_detail.model);
        None
    } else {
        debug!(target: TARGET_LLM_REQUEST, "[{} {} {}]: successfully generated response.", worker_detail.name, worker_detail.id, worker_detail.model);
        Some(response_text)
    }
}
