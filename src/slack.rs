use reqwest::Client;
use serde_json::json;
use tracing::{error, info};

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

    info!(target: "web_request", "Sending Slack notification with payload: {}", payload);
    let res = client
        .post("https://slack.com/api/chat.postMessage")
        .header("Content-Type", "application/json")
        .bearer_auth(slack_token)
        .body(payload.to_string())
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                info!(" ** Slack notification sent successfully");
            } else {
                let error_text = response.text().await.unwrap_or_default();
                error!(" !! Error sending Slack notification: {}", error_text);
                error!(" !! Payload: {}", payload);
            }
        }
        Err(err) => {
            error!(" !! Error sending Slack notification: {:?}", err);
        }
    }
}
