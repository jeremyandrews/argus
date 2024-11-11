use tokio::time::{sleep, Duration};
use tracing::{debug, info};

use crate::LLMClient;
use crate::TARGET_LLM_REQUEST;

pub async fn analysis_loop(worker_id: i16, llm_client: &LLMClient, model: &str) {
    info!(target: TARGET_LLM_REQUEST, "Analysis worker {}: starting analysis_loop.", worker_id);
    debug!(
        "Decision worker {} is running with model '{}' using {:?}.",
        worker_id, model, llm_client
    );
    debug!(
        "Analysis worker {} is running with model '{}' using {:?}.",
        worker_id, model, llm_client
    );

    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
