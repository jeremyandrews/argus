use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::Serialize;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Serialize)]
struct Claims {
    iss: String,
    iat: u64,
}

/// Send iOS app a push notification.
///
/// # Arguments
/// * `title` - The title of the notification.
/// * `body` - The body of the notification.
pub async fn send_to_app(title: &str, body: &str) {
    info!(
        "Preparing to send notification: title = '{}', body = '{}'",
        title, body
    );

    // Load required environment variables, or disable app notifications.
    let team_id = match env::var("APP_TEAM_ID") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_TEAM_ID environment variable not set. App notifications are disabled.");
            return;
        }
    };
    let key_id = match env::var("APP_KEY_ID") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_KEY_ID environment variable not set. App notifications are disabled.");
            return;
        }
    };
    let device_token = match env::var("APP_DEVICE_TOKEN") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_DEVICE_TOKEN environment variable not set. App notifications are disabled.");
            return;
        }
    };
    let private_key_path = match env::var("APP_PRIVATE_KEY_PATH") {
        Ok(val) => val,
        Err(_) => {
            warn!(
                "APP_PRIVATE_KEY_PATH environment variable not set. App notifications are disabled."
            );
            return;
        }
    };

    // Load the private key
    let private_key = match fs::read_to_string(&private_key_path) {
        Ok(key) => key,
        Err(e) => {
            warn!(
                "Failed to read private key file from path '{}': {}. App notifications are disabled.",
                private_key_path, e
            );
            return;
        }
    };

    // Get the current time in seconds since the UNIX epoch
    let start = SystemTime::now();
    let iat = match start.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => {
            error!("System time is before UNIX epoch. App notifications are disabled.");
            return;
        }
    };

    debug!("Generated claims: iss = '{}', iat = {}", team_id, iat);
    let claims = Claims { iss: team_id, iat };

    // Create the JWT header
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id);

    // Encode the token
    let encoding_key = match EncodingKey::from_ec_pem(private_key.as_bytes()) {
        Ok(key) => key,
        Err(e) => {
            warn!(
                "Failed to create encoding key from private key: {}. App notifications are disabled.",
                e
            );
            return;
        }
    };

    let jwt_token = match encode(&header, &claims, &encoding_key) {
        Ok(token) => token,
        Err(e) => {
            warn!(
                "Failed to encode JWT token: {}. App notifications are disabled.",
                e
            );
            return;
        }
    };

    debug!("Generated JWT token: {}", jwt_token);

    // Define the payload
    let payload = serde_json::json!({
        "aps": {
            "alert": {
                "title": title,
                "body": body,
            },
            "sound": "default",
            "badge": 1,
            "content-available": 1
        }
    });

    // Define the APNs endpoint
    let apns_url = format!(
        "https://api.sandbox.push.apple.com/3/device/{}",
        device_token
    );

    let client = match Client::builder().http2_prior_knowledge().build() {
        Ok(client) => client,
        Err(e) => {
            warn!(
                "Failed to build HTTP client: {}. App notifications are disabled.",
                e
            );
            return;
        }
    };

    debug!(
        "Sending POST request to APNs: URL = '{}', payload = {:?}",
        apns_url, payload
    );

    // Send the POST request
    match client
        .post(&apns_url)
        .header("apns-topic", "com.andrews.Argus.Argus")
        .header("apns-priority", "10")
        .header("authorization", format!("bearer {}", jwt_token))
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                info!("Notification sent successfully: {:?}", response);
            } else {
                error!(
                    "Failed to send notification: Status = {}, Response = {:?}",
                    response.status(),
                    response
                );
            }
        }
        Err(e) => {
            error!("Failed to send POST request to APNs: {}", e);
        }
    }
}
