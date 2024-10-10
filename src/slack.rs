use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use crate::TARGET_WEB_REQUEST;

/// Sends the formatted article to the Slack channel.
pub async fn send_to_slack(
    article: &str,
    response: &str,
    slack_token: &str,
    default_channel: &str,
) {
    // Parse the TOPICS environment variable to get the topic-to-channel mappings
    let topics = std::env::var("TOPICS").unwrap().replace('\n', "");
    let topic_mappings: HashMap<&str, &str> = topics
        .split(';')
        .filter_map(|entry| {
            let mut parts = entry.trim().splitn(2, ':');
            if let (Some(topic), Some(channel)) = (parts.next(), parts.next()) {
                Some((topic.trim(), channel.trim()))
            } else {
                None
            }
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
        .copied()
        .unwrap_or(default_channel);

    let summary = response_json["summary"]
        .as_str()
        .unwrap_or("No summary available");
    let tiny_summary = response_json["tiny_summary"]
        .as_str()
        .unwrap_or("No tiny summary available");
    let critical_analysis = response_json["critical_analysis"]
        .as_str()
        .unwrap_or("No critical analysis available");
    let logical_fallacies = response_json["logical_fallacies"]
        .as_str()
        .unwrap_or("No logical fallacies available");
    let relation_to_topic = response_json["relation_to_topic"]
        .as_str()
        .unwrap_or("No relation to topic available");
    let model = response_json["model"].as_str().unwrap_or("Unknown model");

    // First message payload (title, topic, and tiny summary in a block)
    let first_payload = json!({
        "channel": channel,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(
                        "*{}*\n*Topic:* {}\n\n{}",
                        article, topic, tiny_summary
                    )
                }
            }
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });

    // Send the first message and get its 'ts' for threading
    if let Some(slack_response_json) =
        send_slack_message(&client, slack_token, &first_payload, &worker_id).await
    {
        let ts = slack_response_json["ts"].as_str().unwrap_or("").to_string();
        if ts.is_empty() {
            error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to get ts from Slack response", worker_id);
            return;
        }

        // Second message payload (rest of the content)
        let second_payload = json!({
            "channel": channel,
            "thread_ts": ts,
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("{}\n{}", article, topic),
                    }
                },
                {
                    "type": "divider"
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "*Summary*"
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": summary
                    }
                },
                {
                    "type": "divider"
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "*Critical Analysis*"
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": critical_analysis
                    }
                },
                {
                    "type": "divider"
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "*Logical Fallacies*"
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": logical_fallacies
                    }
                },
                {
                    "type": "divider"
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "*Relevance*"
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": relation_to_topic
                    }
                },
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "*Model*"
                    }
                },
                {
                    "type": "section",
                    "text": {
                            "type": "mrkdwn",
                            "text": model
                        }
                    },
                {
                    "type": "divider"
                }
            ],
            "unfurl_links": false,
            "unfurl_media": false,
        });

        // Send the second message in the thread
        let _ = send_slack_message(&client, slack_token, &second_payload, &worker_id).await;
    } else {
        error!(target: TARGET_WEB_REQUEST, "Worker {}: Failed to send first Slack message", worker_id);
        return;
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
                .header("Content-Type", "application/json")
                .bearer_auth(slack_token)
                .body(payload.to_string())
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    debug!(target: TARGET_WEB_REQUEST, "Worker {}: Slack notification sent successfully", worker_id);
                    match response.json::<serde_json::Value>().await {
                        Ok(json) => return Some(json),
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
