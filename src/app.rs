use aws_sdk_s3::config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::{Client, Config};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::Value;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};
use url::Url;
use uuid::Uuid;

/// Send iOS app a push notification.
///
/// # Arguments
/// * `json` - A json object with details about the analyzed article.
/// * `importance` - Notification importance: "high" or "low".
pub async fn send_to_app(json: &Value, importance: &str) -> Option<String> {
    // Upload the JSON to R2
    let json_url = upload_to_r2(json).await?;
    let title = json
        .get("tiny_title")
        .and_then(|v| v.as_str())
        .unwrap_or("No title available.");
    let body = json
        .get("tiny_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("No summary available.");

    // Extract base URL of the article
    let domain = json
        .get("url")
        .and_then(|v| v.as_str())
        .and_then(|url| {
            Url::parse(url)
                .ok()
                .and_then(|parsed_url| parsed_url.host_str().map(|host| host.to_string()))
        })
        .unwrap_or_else(|| "unknown".to_string());

    info!(
        "Sending title=({}) body=({}) domain=({})",
        title, body, domain
    );

    // Load required environment variables
    let service_account_path = match env::var("FIREBASE_SERVICE_ACCOUNT_PATH") {
        Ok(path) => path,
        Err(e) => {
            error!(
                "Environment variable FIREBASE_SERVICE_ACCOUNT_PATH not set: {}",
                e
            );
            return None;
        }
    };
    let service_account = match fs::read_to_string(&service_account_path) {
        Ok(contents) => contents,
        Err(e) => {
            error!(
                "Failed to read Firebase service account file at {}: {}",
                service_account_path, e
            );
            return None;
        }
    };
    let service_account: Value = match serde_json::from_str(&service_account) {
        Ok(json) => json,
        Err(e) => {
            error!("Invalid JSON in Firebase service account file: {}", e);
            return None;
        }
    };
    let client_email = match service_account["client_email"].as_str() {
        Some(email) => email,
        None => {
            error!("Missing 'client_email' field in Firebase service account JSON.");
            return None;
        }
    };
    let private_key = match service_account["private_key"].as_str() {
        Some(key) => key,
        None => {
            error!("Missing 'private_key' field in Firebase service account JSON.");
            return None;
        }
    };
    let project_id = match service_account["project_id"].as_str() {
        Some(id) => id,
        None => {
            error!("Missing 'project_id' field in Firebase service account JSON.");
            return None;
        }
    };

    // Generate JWT token for FCM
    let iat = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(e) => {
            error!("Failed to calculate current time for JWT token: {}", e);
            return None;
        }
    };
    let exp = iat + 3600; // 1-hour validity
    let claims = serde_json::json!({
        "iss": client_email,
        "scope": "https://www.googleapis.com/auth/firebase.messaging",
        "aud": "https://oauth2.googleapis.com/token",
        "iat": iat,
        "exp": exp,
    });
    let encoding_key = match EncodingKey::from_rsa_pem(private_key.as_bytes()) {
        Ok(key) => key,
        Err(e) => {
            error!("Failed to parse RSA private key for JWT token: {}", e);
            return None;
        }
    };
    let jwt_token = match encode(&Header::new(Algorithm::RS256), &claims, &encoding_key) {
        Ok(token) => token,
        Err(e) => {
            error!("Failed to encode JWT token: {}", e);
            return None;
        }
    };
    info!("Successfully generated JWT token.");

    // Get OAuth token
    let oauth_response = match reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer&assertion={}",
            jwt_token
        ))
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to send POST request for OAuth token: {}", e);
            return None;
        }
    };
    if !oauth_response.status().is_success() {
        let status = oauth_response.status();
        let response_text = oauth_response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());
        error!(
            "OAuth token request failed: HTTP Status = {}, Response = {}",
            status, response_text
        );
        return None;
    }
    let oauth_body = match oauth_response.json::<Value>().await {
        Ok(body) => body,
        Err(e) => {
            error!(
                "Failed to parse JSON response from OAuth token request: {}",
                e
            );
            return None;
        }
    };
    let access_token = match oauth_body["access_token"].as_str() {
        Some(token) => token,
        None => {
            error!("OAuth token response missing 'access_token' field.");
            return None;
        }
    };
    info!("Successfully retrieved OAuth token.");

    // Build the notification payload
    let priority = if importance == "high" {
        "high"
    } else {
        "normal"
    };

    println!("priority: {}", priority);
    let payload = serde_json::json!({
        "message": {
            "notification": {
                "title": title,
                "body": body
            },
            "data": {
                "json_url": json_url,
                "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
                "article_title": json.get("title").and_then(|v| v.as_str()).unwrap_or("none"),
                "domain": domain,
            },
            "android": {
                "priority": priority
            },
            "apns": {
                "headers": {
                    "apns-priority": if importance == "high" { "10" } else { "5" }
                },
                "payload": {
                    "aps": {
                        "alert": {
                            "title": title,
                            "body": body
                        },
                        "content-available": 1
                    }
                }
            },
            "token": "dX38ZiPQ5UrNgE8_1j6ASo:APA91bHa9GOHCprsgCXtSanaUIMnYJ5g_jGyUphTmcpAbrw1koqzUpImE0WkBBRvc-hm0IP51rxbKoWvQOUmwpTAKgv-dy64SOuIm28TzH8xYbVB6Xtc9Mc"
        }
    });
    let payload = serde_json::json!({
        "message": {
            "notification": {
                "title": title,
                "body": body,
            },
            "data": {
                "json_url": json_url,
                "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
                "article_title": json.get("title").and_then(|v| v.as_str()).unwrap_or("none"),
                "domain": domain
            },
            "android": {
                "priority": "high"
            },
            "apns": {
                "headers": {
                    "apns-priority": "10"
                },
                "payload": {
                    "aps": {
                        "sound": "default",
                        "badge": 1
                    },
                }
            },
            "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
        }
    });
    let payload = serde_json::json!({
        "message": {
            "data": {
                "json_url": json_url,
                "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
                "article_title": json.get("title").and_then(|v| v.as_str()).unwrap_or("none"),
                "domain": domain,
                "title": title,
                "body": body
            },
            "android": {
                "priority": "high"
            },
            "apns": {
                "headers": {
                    "apns-priority": "10"
                },
                "payload": {
                    "aps": {
                        "sound": "default",
                        "badge": 1
                    }
                }
            },
            "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
        }
    });

    // Send notification via FCM
    let fcm_url = format!(
        "https://fcm.googleapis.com/v1/projects/{}/messages:send",
        project_id
    );

    let client = reqwest::Client::new();
    let response = match client
        .post(&fcm_url)
        .bearer_auth(access_token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to send POST request to FCM: {}", e);
            return None;
        }
    };

    let status = response.status();
    if status.is_success() {
        match response.text().await {
            Ok(text) => {
                info!(
                    "Notification sent successfully. Status = {}, Response = {}",
                    status, text
                );
            }
            Err(e) => {
                warn!(
                    "Notification sent successfully, but failed to read response body. Status = {}, Error = {}",
                    status,
                    e
                );
            }
        }
    } else {
        let response_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());
        error!(
            "Failed to send notification. Status = {}, Response = {}",
            status, response_text
        );
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
