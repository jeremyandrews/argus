use reqwest::Client;
use serde_json::json;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

pub async fn send_to_slack(article: &str, response: &str, slack_token: &str, slack_channel: &str) {
    let client = Client::new();
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
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": response
                }
            }
        ],
        "unfurl_links": false,
        "unfurl_media": false,
    });

    let max_retries = 3;
    let mut backoff = 2;

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
                    info!(" ** Slack notification sent successfully");
                    return;
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    warn!(" !! Error sending Slack notification: {}", error_text);
                    warn!(" !! Payload: {}", payload);
                }
            }
            Ok(Err(err)) => {
                warn!(" !! Error sending Slack notification: {:?}", err);
            }
            Err(_) => {
                warn!(" !! Timeout sending Slack notification");
            }
        }

        if attempt < max_retries - 1 {
            info!(
                "Retrying Slack notification... (attempt {}/{})",
                attempt + 1,
                max_retries
            );
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            backoff *= 2; // Exponential backoff
        } else {
            error!(
                "Failed to send Slack notification after {} attempts",
                max_retries
            );
        }
    }
}
