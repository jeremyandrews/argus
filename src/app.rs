use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::Serialize;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;

#[derive(Serialize)]
struct Claims {
    iss: String,
    iat: u64,
}

/// Sends a push notification to the iOS app.
///
/// # Arguments
/// * `title` - The title of the notification.
/// * `body` - The body of the notification.
pub async fn send_to_app(title: &str, body: &str) {
    // Get configuration from environment variables.
    let team_id = env::var("APP_TEAM_ID").expect("APP_TEAM_ID environment variable not set");
    let key_id = env::var("APP_KEY_ID").expect("APP_KEY_ID environment variable not set");
    let device_token =
        env::var("APP_DEVICE_TOKEN").expect("APP_DEVICE_TOKEN environment variable not set");
    let private_key_path = env::var("APP_PRIVATE_KEY_PATH")
        .expect("APP_PRIVATE_KEY_PATH environment variable not set");

    // Load the private key
    let private_key =
        fs::read_to_string(private_key_path).expect("Failed to read private key file");

    // Get the current time in seconds since the UNIX epoch
    let start = SystemTime::now();
    let iat = start
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    // Create the claims
    let claims = Claims { iss: team_id, iat };

    // Create the JWT header
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id);

    // Encode the token
    let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes())
        .expect("Failed to create encoding key from private key");
    let jwt_token = encode(&header, &claims, &encoding_key).expect("Failed to encode JWT token");

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
    let client = Client::builder()
        .http2_prior_knowledge() // Ensure HTTP/2 is explicitly enabled
        .build()
        .expect("Failed to build HTTP client");

    // Send the POST request
    let response = client
        .post(&apns_url)
        .header("apns-topic", "com.andrews.Argus.Argus")
        .header("apns-priority", "10")
        .header("authorization", format!("bearer {}", jwt_token))
        .header("Content-Type", "application/json")
        .body(payload.to_string())
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("Failed to send POST request to APNs");

    if response.status().is_success() {
        println!("Notification sent successfully: {:#?}", response);
    } else {
        eprintln!("Failed to send notification: {:#?}", response);
    }
}
