use reqwest::{header::HeaderValue, Client};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::TARGET_WEB_REQUEST;

/// Converts standard Markdown to Slack-compatible formatting.
fn deduplicate_markdown(text: &str) -> String {
    text.replace("**", "*") // Convert bold from **text** to *text*
        .replace("__", "*") // Convert bold from __text__ to *text*
        .replace("~~", "~") // Strikethrough remains the same
        .replace("```", "```\n") // Ensure code blocks have a newline after ```
        .replace("`", "`") // Inline code remains the same
        .replace("# ", "*") // Convert H1 headers to bold
        .replace("## ", "*") // Convert H2 headers to bold
        .replace("### ", "*") // Convert H3 headers to bold
        .replace("> ", ">") // Blockquotes remain the same
        .replace("- ", "• ") // Convert unordered list dashes to bullets
        .replace("* ", "• ") // Convert unordered list asterisks to bullets
        .replace("\n", "\n") // Ensure newlines are preserved
}

/// Sends the formatted article to the Slack channel.
pub async fn send_to_slack(
    article: &str,
    response: &str,
    slack_token: &str,
    default_channel: &str,
) {
    // Parse the TOPICS environment variable to get the topic-to-channel mappings
    let topics = std::env::var("TOPICS").unwrap().replace('\n', "");
    let topic_mappings: HashMap<&str, (&str, Option<&str>)> = topics
        .split(';')
        .filter_map(|entry| {
            let mut parts = entry.trim().splitn(3, ':');
            let topic_name = parts.next()?.trim();
            let topic_prompt = parts.next()?.trim();
            let channel = parts.next().map(|ch| ch.trim()); // Optional channel
            Some((topic_name, (topic_prompt, channel)))
        })
        .collect();

    let client = Client::new();
    let worker_id = format!("{:?}", std::thread::current().id()); // Retrieve the worker number

    // Parse response JSON
    debug!(target: TARGET_WEB_REQUEST, "Worker {}: Parsing response JSON", worker_id);
    let response_json: serde_json::Value = match serde_json::from_str(response) {
        Ok(json) => json,
        Err(err) => {
            error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to parse response JSON: {:?}", worker_id, err);
            return;
        }
    };

    let topic = response_json["topic"]
        .as_str()
        .unwrap_or("No topic available")
        .trim();

    // Determine the Slack channel based on the matched topic
    let channel = topic_mappings
        .get(topic)
        .and_then(|(_, channel)| *channel)
        .unwrap_or(default_channel);

    let summary = deduplicate_markdown(
        response_json["summary"]
            .as_str()
            .unwrap_or("No summary available"),
    );
    let tiny_summary = deduplicate_markdown(
        response_json["tiny_summary"]
            .as_str()
            .unwrap_or("No tiny summary available"),
    );
    let critical_analysis = deduplicate_markdown(
        response_json["critical_analysis"]
            .as_str()
            .unwrap_or("No critical analysis available"),
    );
    let logical_fallacies = deduplicate_markdown(
        response_json["logical_fallacies"]
            .as_str()
            .unwrap_or("No logical fallacies available"),
    );
    let relation_to_topic = deduplicate_markdown(
        response_json["relation_to_topic"]
            .as_str()
            .unwrap_or("No relation to topic available"),
    );
    let source_analysis = deduplicate_markdown(
        response_json["source_analysis"]
            .as_str()
            .unwrap_or("No source analysis available"),
    );
    let model = deduplicate_markdown(response_json["model"].as_str().unwrap_or("Unknown model"));
    let elapsed_time = response_json["elapsed_time"].as_f64().unwrap_or(0.0);

    // First message payload (title, topic, and tiny summary in a block)
    let first_payload = json!({
        "channel": channel,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("{}\n{}", article, tiny_summary),
                }
            },
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });
    info!(target: TARGET_WEB_REQUEST, "Worker {}: Payload size: {} characters", worker_id, first_payload.to_string().len());

    // Send the first message and get its 'ts' for threading
    if let Some(slack_response_json) =
        send_slack_message(&client, slack_token, &first_payload, &worker_id).await
    {
        let ts = slack_response_json["ts"].as_str().unwrap_or("").to_string();
        if ts.is_empty() {
            error!(target: TARGET_WEB_REQUEST, "Worker {}: Invalid ts returned from Slack", worker_id);
        } else {
            debug!(target: TARGET_WEB_REQUEST, "Worker {}: Using thread ts: {}", worker_id, ts);
        }

        // Build individual blocks for each section with dividers
        let mut blocks: Vec<Value> = vec![];

        if !relation_to_topic.is_empty() {
            add_section_with_divider(
                &mut blocks,
                format!(
                    "*Relevance*\n{}\n\n_Generated with *{}* in {:.2} seconds._\n",
                    relation_to_topic, model, elapsed_time
                ),
            );
        }

        if !summary.is_empty() {
            add_section_with_divider(&mut blocks, format!("*Summary*\n{}", summary));
        }

        if !critical_analysis.is_empty() {
            add_section_with_divider(
                &mut blocks,
                format!("*Critical Analysis*\n{}", critical_analysis),
            );
        }

        if !logical_fallacies.is_empty() {
            add_section_with_divider(
                &mut blocks,
                format!("*Logical Fallacies*\n{}", logical_fallacies),
            );
        }

        if !source_analysis.is_empty() {
            add_section_with_divider(
                &mut blocks,
                format!("*Source Analysis*\n{}", source_analysis),
            );
        }

        if !article.is_empty() {
            add_section_with_divider(&mut blocks, article.to_string());
        }

        let thread_payload = json!({
            "channel": channel,
            "thread_ts": ts,
            "blocks": blocks,
            "unfurl_links": true,
            "unfurl_media": true,
        });
        info!(target: TARGET_WEB_REQUEST, "Worker {}: Thread payload size: {} characters", worker_id, thread_payload.to_string().len());

        // Send the second message in the thread
        if let Some(slack_response_json) =
            send_slack_message(&client, slack_token, &thread_payload, &worker_id).await
        {
            info!(
                "Worker {}: Slack accepted message: {}",
                worker_id, slack_response_json
            );
        } else {
            error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to send Slack message", worker_id);
        }
    } else {
        error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to send first Slack message", worker_id);
    }
}

// Function to add a section followed by a divider
fn add_section_with_divider(blocks: &mut Vec<serde_json::Value>, section_text: String) {
    blocks.push(json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": section_text
        }
    }));
    blocks.push(json!({
        "type": "divider"
    }));
}

// Helper function to send a Slack message with retries
async fn send_slack_message(
    client: &Client,
    slack_token: &str,
    payload: &serde_json::Value,
    worker_id: &str,
) -> Option<serde_json::Value> {
    let max_retries = 3;
    let mut backoff = 2;
    let max_backoff = 32; // Maximum backoff time in seconds

    for attempt in 0..max_retries {
        info!(target: TARGET_WEB_REQUEST, "Worker {}: Sending Slack notification with payload: {}", worker_id, payload);

        match timeout(
            Duration::from_secs(30),
            client
                .post("https://slack.com/api/chat.postMessage")
                .header("Content-Type", "application/json; charset=utf-8")
                .bearer_auth(slack_token)
                .body(payload.to_string())
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                let default_header = HeaderValue::from_static("unknown");
                let limit = response
                    .headers()
                    .get("X-RateLimit-Limit")
                    .unwrap_or(&default_header);
                let remaining = response
                    .headers()
                    .get("X-RateLimit-Remaining")
                    .unwrap_or(&default_header);
                let reset = response
                    .headers()
                    .get("X-RateLimit-Reset")
                    .unwrap_or(&default_header);
                info!(target: TARGET_WEB_REQUEST, "Worker {}: Rate limit: {}, remaining: {}, reset: {}", worker_id, limit.to_str().unwrap_or("unknown"), remaining.to_str().unwrap_or("unknown"), reset.to_str().unwrap_or("unknown"));

                if response.status().is_success() {
                    debug!(target: TARGET_WEB_REQUEST, "Worker {}: Slack notification sent successfully", worker_id);
                    match response.json::<serde_json::Value>().await {
                        Ok(json) => {
                            info!(target: TARGET_WEB_REQUEST, "Worker {}: Slack response: {:?}", worker_id, json);
                            return Some(json);
                        }
                        Err(err) => {
                            error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to parse Slack response JSON: {:?}", worker_id, err);
                            return None;
                        }
                    }
                } else {
                    let error_text = match response.text().await {
                        Ok(text) => text,
                        Err(_) => "Unknown error".to_string(),
                    };
                    warn!(target: TARGET_WEB_REQUEST, "Worker {}: Error sending Slack notification: {}", worker_id, error_text);
                    warn!(target: TARGET_WEB_REQUEST, "Worker {}: Payload: {}", worker_id, payload);
                }
            }
            Ok(Err(err)) => {
                warn!(target: TARGET_WEB_REQUEST, "Worker {}: Error sending Slack notification: {:?}", worker_id, err);
            }
            Err(_) => {
                warn!(target: TARGET_WEB_REQUEST, "Worker {}: Timeout sending Slack notification", worker_id);
            }
        }

        if attempt < max_retries - 1 {
            info!(
                target: TARGET_WEB_REQUEST,
                "Worker {}: Retrying Slack notification... (attempt {}/{})",
                worker_id,
                attempt + 1,
                max_retries
            );
            debug!(target: TARGET_WEB_REQUEST, "Worker {}: Backing off for {} seconds before retry", worker_id, backoff.min(max_backoff));
            tokio::time::sleep(Duration::from_secs(backoff.min(max_backoff))).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                target: TARGET_WEB_REQUEST,
                "Worker {}: Failed to send Slack notification after {} attempts",
                worker_id,
                max_retries
            );
        }
    }

    None
}
