use aws_sdk_s3::config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::{Client, Config};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::Value;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{error, info};
use url::Url;
use uuid::Uuid;

use crate::db::Database;

#[derive(Serialize)]
struct Claims {
    iss: String,
    iat: u64,
}

/// Send iOS app a push notification.
///
/// # Arguments
/// * `json` - A json object with details about the analyzed article.
pub async fn send_to_app(json: &Value) -> Option<String> {
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
    let pub_date = json
        .get("pub_date")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown date");

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

    // Load required environment variables
    let team_id = env::var("APP_TEAM_ID").ok()?;
    let key_id = env::var("APP_KEY_ID").ok()?;
    let private_key_path = env::var("APP_PRIVATE_KEY_PATH").ok()?;
    let private_key = fs::read_to_string(&private_key_path).ok()?;

    // Generate JWT token
    let iat = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let claims = Claims {
        iss: team_id.clone(),
        iat,
    };
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id.clone());
    let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes()).ok()?;
    let jwt_token = encode(&header, &claims, &encoding_key).ok()?;

    // Send notification
    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .build()
        .ok()?;

    // Fetch subscribed devices with their priorities
    let db = Database::instance().await;
    let topic = json.get("topic").and_then(|v| v.as_str()).unwrap_or("none");
    let device_tokens_with_priorities = db.fetch_devices_for_topic(topic).await.ok()?;

    for (device_token, priority) in device_tokens_with_priorities {
        let importance = if priority == "high" { "high" } else { "low" };
        let apns_priority = if importance == "high" { "10" } else { "5" };

        // Determine payload based on importance
        let payload = serde_json::json!({
            "aps": {
                "alert": if importance == "high" {
                    serde_json::json!({ "title": title, "body": body })
                } else {
                    serde_json::Value::Null
                },
                "sound": if importance == "high" {
                    serde_json::Value::String("default".to_string())
                } else {
                    serde_json::Value::Null
                },
                "content-available": 1
            },
            "data": {
                "json_url": json_url,
                "topic": json.get("topic").and_then(|v| v.as_str()).unwrap_or("none"),
                "article_title": json.get("title").and_then(|v| v.as_str()).unwrap_or("none"),
                "title": if importance != "high" { Some(title) } else { None },
                "body": if importance != "high" { Some(body) } else { None },
                "affected": json.get("affected"),
                "domain": domain,
                "pub_date": pub_date
            }
        });

        let apns_url = format!("https://api.push.apple.com/3/device/{}", device_token);

        match client
            .post(&apns_url)
            .header("apns-topic", "com.andrews.Argus.Argus")
            .header("apns-priority", apns_priority)
            .header("authorization", format!("bearer {}", jwt_token))
            .header("Content-Type", "application/json")
            .body(payload.to_string())
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                info!("Notification sent successfully.");
            }
            Ok(response) => {
                let status = response.status();
                let response_text = response.text().await.unwrap_or_default();
                error!(
                    "Failed to send notification: Status = {}, Response = {}",
                    status, response_text
                );
                if status == StatusCode::GONE || status == StatusCode::NOT_FOUND {
                    info!(
                        "Device token {} is unregistered. Unsubscribing from all topics.",
                        device_token
                    );
                    if let Err(e) = db.remove_device_token(&device_token).await {
                        error!("Failed to unsubscribe device {}: {}", device_token, e);
                    } else {
                        info!("Device {} unsubscribed from all topics.", device_token);
                    }
                }
            }
            Err(e) => {
                error!("Failed to send POST request to APNs: {}", e);
            }
        }
    }
    Some(json_url)
}

pub async fn upload_to_r2(json: &Value) -> Option<String> {
    let bucket_name = env::var("R2_BUCKET_NAME").ok()?;
    let endpoint_url = env::var("R2_ENDPOINT_URL").ok()?;
    let public_url = env::var("R2_PUBLIC_URL").ok()?;
    let access_key = env::var("R2_ACCESS_KEY_ID").ok()?;
    let secret_key = env::var("R2_SECRET_ACCESS_KEY").ok()?;

    let creds = Credentials::new(access_key, secret_key, None, None, "custom");
    let config = Config::builder()
        .region(Region::new("us-east-1"))
        .endpoint_url(&endpoint_url)
        .credentials_provider(creds)
        .behavior_version(BehaviorVersion::latest())
        .build();

    let client = Client::from_conf(config);

    let file_name = format!("{}.json", Uuid::new_v4());
    let json_data = json.to_string();

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
            Some(file_url)
        }
        Err(e) => {
            eprintln!("Upload failed with error: {:?}", e);
            None
        }
    }
}
