use reqwest::Client;
use serde_json::json;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

/// Sends the formatted article to the Slack channel.
pub async fn send_to_slack(article: &str, response: &str, slack_token: &str, slack_channel: &str) {
    let client = Client::new();

    // Parse response JSON
    let response_json: serde_json::Value = match serde_json::from_str(response) {
        Ok(json) => json,
        Err(err) => {
            error!("Failed to parse response JSON: {:?}", err);
            return;
        }
    };

    let summary = response_json["summary"]
        .as_str()
        .unwrap_or("No summary available");
    let critical_analysis = response_json["critical_analysis"]
        .as_str()
        .unwrap_or("No critical analysis available");
    let logical_fallacies = response_json["logical_fallacies"]
        .as_str()
        .unwrap_or("No logical fallacies available");
    let relation_to_topic = response_json["relation_to_topic"]
        .as_str()
        .unwrap_or("No relation to topic available");

    let payload = json!({
        "channel": slack_channel,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": article
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
                    "text": "*Relevance to Topic*"
                }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": relation_to_topic
                }
            }
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });

    let max_retries = 3;
    let mut backoff = 2;
    let max_backoff = 32; // Maximum backoff time in seconds

    for attempt in 0..max_retries {
        info!(target: "web_request", "Sending Slack notification with payload: {}", payload);
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
                    info!("** Slack notification sent successfully");
                    return;
                } else {
                    let error_text = match response.text().await {
                        Ok(text) => text,
                        Err(_) => "Unknown error".to_string(),
                    };
                    warn!("!! Error sending Slack notification: {}", error_text);
                    warn!("!! Payload: {}", payload);
                }
            }
            Ok(Err(err)) => {
                warn!("!! Error sending Slack notification: {:?}", err);
            }
            Err(_) => {
                warn!("!! Timeout sending Slack notification");
            }
        }

        if attempt < max_retries - 1 {
            info!(
                "Retrying Slack notification... (attempt {}/{})",
                attempt + 1,
                max_retries
            );
            tokio::time::sleep(Duration::from_secs(backoff.min(max_backoff))).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                "Failed to send Slack notification after {} attempts",
                max_retries
            );
        }
    }
}
