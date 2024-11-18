use async_openai::types::CreateCompletionRequestArgs;
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::options::GenerationOptions;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use crate::TARGET_LLM_REQUEST;
use crate::{LLMClient, LLMParams};

pub async fn generate_llm_response(prompt: &str, params: &LLMParams) -> Option<String> {
    let max_retries = 5;
    let mut response_text = String::new();
    let mut backoff = 2;
    let worker_id = format!("{:?}", std::thread::current().id()); // Retrieve the worker number
    let model = params.model.clone();

    debug!(
        target: TARGET_LLM_REQUEST,
        worker_id = worker_id,
        llm_model = model,
        "Starting LLM response generation for prompt"
    );

    for retry_count in 0..max_retries {
        match params.llm_client {
            LLMClient::Ollama(ref ollama) => {
                let mut request =
                    GenerationRequest::new(params.model.to_string(), prompt.to_string());
                request.options =
                    Some(GenerationOptions::default().temperature(params.temperature));

                debug!(
                    target: TARGET_LLM_REQUEST,
                    worker_id = worker_id,
                    llm_model = model,
                    "Sending Ollama LLM request with prompt"
                );

                match timeout(Duration::from_secs(120), ollama.generate(request)).await {
                    Ok(Ok(response)) => {
                        response_text = response.response;
                        debug!(
                            target: TARGET_LLM_REQUEST,
                            worker_id = worker_id,
                            llm_model = model,
                            response = response_text,
                            "Ollama LLM response received"
                        );
                        break;
                    }
                    Ok(Err(e)) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            worker_id = worker_id,
                            llm_model = model,
                            error = ?e,
                            "Error generating Ollama response"
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            worker_id = worker_id,
                            llm_model = model,
                            "Ollama LLM request timed out"
                        );
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

                debug!(
                    target: TARGET_LLM_REQUEST,
                    worker_id = worker_id,
                    llm_model = model,
                    "Sending OpenAI LLM request with prompt"
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
                                worker_id = worker_id,
                                llm_model = model,
                                response = response_text,
                                "OpenAI LLM response received"
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            worker_id = worker_id,
                            llm_model = model,
                            error = ?e,
                            "Error generating OpenAI response"
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: TARGET_LLM_REQUEST,
                            worker_id = worker_id,
                            llm_model = model,
                            "OpenAI LLM request timed out"
                        );
                    }
                }
            }
        }

        if retry_count < max_retries - 1 {
            info!(
                target: TARGET_LLM_REQUEST,
                worker_id = worker_id,
                llm_model = model,
                backoff_seconds = backoff,
                "Backing off for {} seconds before retry",
                backoff
            );
            sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                target: TARGET_LLM_REQUEST,
                worker_id = worker_id,
                llm_model = model,
                "Failed to generate response after {} retries", max_retries
            );
        }
    }

    if response_text.is_empty() {
        error!(
            target: TARGET_LLM_REQUEST,
            worker_id = worker_id,
            llm_model = model,
            "No response generated after all retries"
        );
        None
    } else {
        debug!(
            target: TARGET_LLM_REQUEST,
            worker_id = worker_id,
            llm_model = model,
            "Successfully generated response"
        );
        Some(response_text)
    }
}
