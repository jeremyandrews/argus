use regex::Regex;
use reqwest::{header::HeaderValue, Client};
use serde_json::json;
use std::collections::HashMap;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::TARGET_WEB_REQUEST;

/// Converts standard Markdown to Slack-compatible formatting.
fn deduplicate_markdown(text: &str) -> String {
    let mut output = text.to_string();

    // Convert bold (**text**) to Slack-supported *text*
    let bold_asterisks_regex = Regex::new(r"\*\*(.*?)\*\*").unwrap();
    output = bold_asterisks_regex
        .replace_all(&output, "*$1*")
        .to_string();

    // Convert bold (__text__) to Slack-supported *text*
    let bold_underscores_regex = Regex::new(r"__(.*?)__").unwrap();
    output = bold_underscores_regex
        .replace_all(&output, "*$1*")
        .to_string();

    // Convert strikethrough (~~text~~) to Slack-supported ~text~
    let strikethrough_regex = Regex::new(r"~~(.*?)~~").unwrap();
    output = strikethrough_regex.replace_all(&output, "~$1~").to_string();

    // Ensure code blocks are properly formatted (no extra newlines)
    let code_block_regex = Regex::new(r"```(\s*\n)?(.*?)```").unwrap();
    output = code_block_regex
        .replace_all(&output, "```\n$2\n```")
        .to_string();

    // Convert Markdown headers (###, ##, #) to bold text
    let header_regex = Regex::new(r"(?m)^(#{1,3})\s*(.+)$").unwrap();
    output = header_regex.replace_all(&output, "*$2*").to_string();

    // Convert list items (- or *) to proper bullets (•)
    let bullet_regex = Regex::new(r"(?m)^\s*[-*]\s+").unwrap();
    output = bullet_regex.replace_all(&output, "• ").to_string();

    // Convert [text](link) to Slack's <link|text> format
    let link_regex = Regex::new(r"\[(.*?)\]\((.*?)\)").unwrap();
    output = link_regex.replace_all(&output, "<$2|$1>").to_string();

    // Ensure proper newline spacing
    output = output.replace("\r\n", "\n").replace("\r", "\n");

    // Clean up any accidental multiple bold markers
    let cleanup_regex = Regex::new(r"\*{3,}(.+?)\*{3,}").unwrap();
    output = cleanup_regex.replace_all(&output, "*$1*").to_string();

    output
}

/// Sends the formatted article to the Slack channel.
pub async fn send_to_slack(
    article: &str,
    response: &str,
    slack_token: &str,
    default_channel: &str,
) {
    let topics = std::env::var("TOPICS")
        .unwrap_or_default()
        .replace('\n', "");
    let topic_mappings: HashMap<&str, (&str, Option<&str>)> = topics
        .split(';')
        .filter_map(|entry| {
            let mut parts = entry.trim().splitn(3, ':');
            let topic_name = parts.next()?.trim();
            let topic_prompt = parts.next()?.trim();
            let channel = parts.next().map(|ch| ch.trim());
            Some((topic_name, (topic_prompt, channel)))
        })
        .collect();

    let client = Client::new();
    let worker_id = format!("{:?}", std::thread::current().id());

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
    let channel = topic_mappings
        .get(topic)
        .and_then(|(_, channel)| *channel)
        .unwrap_or(default_channel);

    let tiny_title = deduplicate_markdown(response_json["tiny_title"].as_str().unwrap_or(""));
    let tiny_summary = deduplicate_markdown(response_json["tiny_summary"].as_str().unwrap_or(""));
    let summary = deduplicate_markdown(response_json["summary"].as_str().unwrap_or(""));
    let critical_analysis =
        deduplicate_markdown(response_json["critical_analysis"].as_str().unwrap_or(""));
    let logical_fallacies =
        deduplicate_markdown(response_json["logical_fallacies"].as_str().unwrap_or(""));
    let relation_to_topic =
        deduplicate_markdown(response_json["relation_to_topic"].as_str().unwrap_or(""));
    let source_analysis =
        deduplicate_markdown(response_json["source_analysis"].as_str().unwrap_or(""));
    let additional_insights = deduplicate_markdown(
        response_json["additional_insights"]
            .as_str()
            .unwrap_or("")
            .trim(),
    );
    info!(target: TARGET_WEB_REQUEST,
        "Worker {}: Additional insights before processing: {:?}",
        worker_id,
        response_json["additional_insights"]
    );
    info!(target: TARGET_WEB_REQUEST,
        "Worker {}: Additional insights after processing: {:?}",
        worker_id,
        additional_insights
    );

    let model = response_json["model"]
        .as_str()
        .unwrap_or("Unknown model")
        .to_string();
    let elapsed_time = response_json["elapsed_time"].as_f64().unwrap_or(0.0);

    // **Step 1: Send the initial message**
    let first_payload = json!({
        "channel": channel,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("*{}*\n{}", tiny_title, tiny_summary),
                }
            },
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });

    if let Some(slack_response_json) =
        send_slack_message(&client, slack_token, &first_payload, &worker_id).await
    {
        let ts = slack_response_json["ts"].as_str().unwrap_or("").to_string();
        if ts.is_empty() {
            error!(target: TARGET_WEB_REQUEST, "Worker {}: Invalid ts returned from Slack", worker_id);
            return;
        }

        // **Step 2: Send remaining content block by block in the thread**
        let sections = vec![
            ("*Article*", article.to_string()),
            ("*Relevance*", relation_to_topic),
            ("*Summary*", summary),
            ("*Critical Analysis*", critical_analysis),
            ("*Logical Fallacies*", logical_fallacies),
            ("*Source Analysis*", source_analysis),
            ("*Argus Speaks*", additional_insights),
        ];

        for (title, content) in sections {
            if !content.is_empty() {
                let thread_payload = json!({
                    "channel": channel,
                    "thread_ts": ts,
                    "blocks": [
                        { "type": "divider" },
                        {
                            "type": "section",
                            "text": { "type": "mrkdwn", "text": format!("{}\n{}", title, content) }
                        }
                    ],
                    "unfurl_links": true,
                    "unfurl_media": false,
                });

                send_slack_message(&client, slack_token, &thread_payload, &worker_id).await;
                tokio::time::sleep(Duration::from_secs(1)).await; // Small delay to prevent rate limiting
            }
        }

        // **Step 3: Send final block with model details**
        let final_payload = json!({
            "channel": channel,
            "thread_ts": ts,
            "blocks": [
                { "type": "divider" },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("_Generated using model _{} in {:.2} seconds._", model, elapsed_time)
                    }
                }
            ],
            "unfurl_links": false,
            "unfurl_media": false,
        });

        send_slack_message(&client, slack_token, &final_payload, &worker_id).await;
    }
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
