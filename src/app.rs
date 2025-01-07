use aws_sdk_s3::config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::{Client, Config};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    iss: String,
    iat: u64,
}

/// Send iOS app a push notification.
///
/// # Arguments
/// * `json` - A json object with details about the analyzed article.
/// * `importance` - Notification importance: "high" or "low".
pub async fn send_to_app(json: &Value, importance: &str) -> Option<String> {
    // Upload the JSON to R2
    let json_url = match upload_to_r2(json).await {
        Some(url) => url,
        None => {
            warn!("Failed to upload JSON to R2. Aborting notification.");
            return None;
        }
    };

    let title = json
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("No title available.");
    let body = json
        .get("tiny_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("No summary available.");

    info!(
        "Preparing to send notification: title = '{}', body = '{}', json_url = '{}'",
        title, body, json_url
    );

    // Load required environment variables, or disable app notifications.
    let team_id = match env::var("APP_TEAM_ID") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_TEAM_ID environment variable not set. App notifications are disabled.");
            return None;
        }
    };
    let key_id = match env::var("APP_KEY_ID") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_KEY_ID environment variable not set. App notifications are disabled.");
            return None;
        }
    };
    let device_token = match env::var("APP_DEVICE_TOKEN") {
        Ok(val) => val,
        Err(_) => {
            warn!("APP_DEVICE_TOKEN environment variable not set. App notifications are disabled.");
            return None;
        }
    };
    let private_key_path = match env::var("APP_PRIVATE_KEY_PATH") {
        Ok(val) => val,
        Err(_) => {
            warn!(
                "APP_PRIVATE_KEY_PATH environment variable not set. App notifications are disabled."
            );
            return None;
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
            return None;
        }
    };

    // Get the current time in seconds since the UNIX epoch
    let start = SystemTime::now();
    let iat = match start.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => {
            error!("System time is before UNIX epoch. App notifications are disabled.");
            return None;
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
            return None;
        }
    };

    let jwt_token = match encode(&header, &claims, &encoding_key) {
        Ok(token) => token,
        Err(e) => {
            warn!(
                "Failed to encode JWT token: {}. App notifications are disabled.",
                e
            );
            return None;
        }
    };

    debug!("Generated JWT token: {}", jwt_token);

    // Determine priority based on importance
    let priority = match importance {
        "high" => "10", // Immediate delivery
        "low" => "5",   // Background delivery
        _ => "5",       // Default to background delivery
    };

    // Define the payload
    let payload = serde_json::json!({
        "aps": {
            "alert": json_alert(title, body, importance == "high"),
            "sound": json_string("default", importance == "high"),
            "badge": json_number(1, importance == "high"),
            "content-available": json_number(1, true), // Always included
        },
        "data": {
            "json_url": json_url,
            "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
        }
    });

    // Define the APNs endpoint
    let apns_url = format!(
        "https://api.sandbox.push.apple.com/3/device/{}",
        device_token
    );

    let client = match reqwest::Client::builder().http2_prior_knowledge().build() {
        Ok(client) => client,
        Err(e) => {
            warn!(
                "Failed to build HTTP client: {}. App notifications are disabled.",
                e
            );
            return None;
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
        .header("apns-priority", priority)
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
    Some(json_url)
}

pub async fn upload_to_r2(json: &Value) -> Option<String> {
    // Load environment variables
    let bucket_name = env::var("R2_BUCKET_NAME").ok()?;
    let endpoint_url = env::var("R2_ENDPOINT_URL").ok()?;
    let public_url = env::var("R2_PUBLIC_URL").ok()?;
    let access_key = env::var("R2_ACCESS_KEY_ID").ok()?;
    let secret_key = env::var("R2_SECRET_ACCESS_KEY").ok()?;

    // Configure the AWS S3 client for R2
    let creds = Credentials::new(access_key, secret_key, None, None, "custom");
    let config = Config::builder()
        .region(Region::new("us-east-1")) // Use the appropriate region
        .endpoint_url(&endpoint_url) // Set the custom endpoint URL
        .credentials_provider(creds)
        .behavior_version(BehaviorVersion::latest()) // Use the correct type for behavior version
        .build();

    let client = Client::from_conf(config);

    // Generate a UUID for the file name
    let file_name = format!("{}.json", Uuid::new_v4());
    let json_data = json.to_string();

    // Attempt to upload the JSON to R2
    match client
        .put_object()
        .bucket(&bucket_name)
        .key(&file_name)
        .body(ByteStream::from(json_data.into_bytes()))
        .content_type("application/json")
        .send()
        .await
    {
        Ok(_) => {
            let file_url = format!("{}/{}", public_url, file_name);
            println!("Upload successful! File URL: {}", file_url);
            Some(file_url) // Return the public URL if successful
        }
        Err(e) => {
            eprintln!("Upload failed with error: {:?}", e);
            None // Return None if upload fails
        }
    }
}

fn json_string(value: &str, condition: bool) -> serde_json::Value {
    if condition {
        serde_json::Value::String(value.to_string())
    } else {
        serde_json::Value::Null
    }
}

fn json_number(value: i64, condition: bool) -> serde_json::Value {
    if condition {
        serde_json::Value::Number(value.into())
    } else {
        serde_json::Value::Null
    }
}

fn json_alert(title: &str, body: &str, condition: bool) -> serde_json::Value {
    if condition {
        serde_json::json!({
            "title": title,
            "body": body,
        })
    } else {
        serde_json::Value::Null
    }
}
