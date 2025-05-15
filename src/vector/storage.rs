use anyhow::Result;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::vectors::VectorsOptions;
use qdrant_client::qdrant::{
    PointId, PointStruct, UpsertPoints, WithPayloadSelector, WithVectorsSelector, WriteOrdering,
};
use qdrant_client::Qdrant;
use serde_json::json;
use std::collections::HashMap;
use tracing::{error, info};

use crate::vector::{QDRANT_URL_ENV, TARGET_VECTOR};

/// Store an article's embedding vector in Qdrant
pub async fn store_embedding(
    sqlite_id: i64,
    embedding: &Vec<f32>,
    published_date: Option<&str>,
    category: Option<&str>,
    quality: i8,
    entity_ids: Option<Vec<i64>>,
    event_date: Option<&str>,
) -> Result<()> {
    info!(target: TARGET_VECTOR, "store_embedding: embedding length = {}", embedding.len());

    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    let mut payload: HashMap<String, qdrant_client::qdrant::Value> = HashMap::new();

    // Add basic metadata - only when present
    if let Some(date) = published_date {
        payload.insert(
            "published_date".to_string(),
            json!(date).try_into().unwrap(),
        );
    }

    payload.insert("category".to_string(), json!(category).try_into().unwrap());
    payload.insert(
        "quality_score".to_string(),
        json!(quality).try_into().unwrap(),
    );

    // Add entity IDs if available
    if let Some(ids) = entity_ids {
        if !ids.is_empty() {
            payload.insert("entity_ids".to_string(), json!(ids).try_into().unwrap());
        }
    }

    // Add event date if available
    if let Some(date) = event_date {
        if !date.is_empty() {
            payload.insert("event_date".to_string(), json!(date).try_into().unwrap());
        }
    }

    info!(target: TARGET_VECTOR, "store_embedding: payload for article {}: {:?}", sqlite_id, payload);

    let point = PointStruct {
        id: Some(PointId {
            point_id_options: Some(PointIdOptions::Num(
                sqlite_id
                    .try_into()
                    .expect("SQLite ID should never be negative"),
            )),
        }),
        vectors: Some(qdrant_client::qdrant::Vectors {
            vectors_options: Some(VectorsOptions::Vector(qdrant_client::qdrant::Vector {
                data: embedding.clone(),
                indices: None,
                vector: None,
                vectors_count: None,
            })),
        }),
        payload,
        ..Default::default()
    };

    let upsert_points = UpsertPoints {
        collection_name: "articles".to_string(),
        points: vec![point],
        wait: Some(true),
        ordering: Some(WriteOrdering::default()),
        shard_key_selector: None,
    };

    match client.upsert_points(upsert_points).await {
        Ok(_) => {
            info!(
                target: TARGET_VECTOR,
                "Successfully stored embedding for article {}", sqlite_id
            );
            Ok(())
        }
        Err(e) => {
            error!(
                target: TARGET_VECTOR,
                "Failed to store embedding for article {}: {:?}", sqlite_id, e
            );
            Err(anyhow::anyhow!("Failed to store embedding: {:?}", e))
        }
    }
}

/// Retrieve an article's vector embedding from Qdrant
pub async fn get_article_vector_from_qdrant(article_id: i64) -> Result<Vec<f32>> {
    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    info!(target: TARGET_VECTOR, "Retrieving vector for article {}", article_id);

    // Get the article's vector from Qdrant
    let response = client
        .get_points(qdrant_client::qdrant::GetPoints {
            collection_name: "articles".to_string(),
            ids: vec![PointId {
                point_id_options: Some(PointIdOptions::Num(article_id as u64)),
            }],
            with_payload: Some(WithPayloadSelector::from(false)),
            with_vectors: Some(WithVectorsSelector::from(true)),
            ..Default::default()
        })
        .await?;

    info!(target: TARGET_VECTOR, "Vector retrieval response received for article {}, points: {}", 
          article_id, response.result.len());

    // Extract the vector from the response
    if let Some(point) = response.result.first() {
        info!(target: TARGET_VECTOR, "Found point for article {}", article_id);

        if let Some(vectors) = &point.vectors {
            info!(target: TARGET_VECTOR, "Vector data exists for article {}", article_id);

            // Get the specific type name of the vectors_options enum
            let type_name = std::any::type_name_of_val(&vectors.vectors_options);
            info!(target: TARGET_VECTOR, "Vector options type for article {}: {}", article_id, type_name);

            if let Some(opts) = &vectors.vectors_options {
                // Detailed debug info about the enum variant
                info!(target: TARGET_VECTOR, "Vector options variant for article {}: {:?}", article_id, opts);

                // Extract the vector data
                match opts {
                    &qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(ref v) => {
                        info!(target: TARGET_VECTOR, 
                            "Successfully extracted vector for article {}: dimensions={}, first_values=[{:.4}, {:.4}, ...]", 
                            article_id, v.data.len(), 
                            v.data.first().unwrap_or(&0.0), 
                            v.data.get(1).unwrap_or(&0.0));

                        let magnitude = v.data.iter().map(|x| x * x).sum::<f32>().sqrt();
                        info!(target: TARGET_VECTOR, 
                            "Vector magnitude for article {}: {:.6}", article_id, magnitude);

                        // Check for problematic vectors
                        if magnitude < 0.001 {
                            error!(target: TARGET_VECTOR, 
                                "CRITICAL: Near-zero magnitude vector detected for article {}", article_id);
                            return Err(anyhow::anyhow!("Near-zero magnitude vector detected"));
                        }

                        return Ok(v.data.clone());
                    }
                    other => {
                        error!(target: TARGET_VECTOR, 
                            "Unexpected vector format for article {}: {:?}", article_id, other);
                        return Err(anyhow::anyhow!("Unexpected vector format"));
                    }
                }
            } else {
                error!(target: TARGET_VECTOR, "Vector options is None for article {}", article_id);
            }
        } else {
            error!(target: TARGET_VECTOR, "No vectors in response for article {}", article_id);
        }
    } else {
        error!(target: TARGET_VECTOR, "Empty result for article {} vector retrieval", article_id);
    }

    error!(target: TARGET_VECTOR, "Failed to retrieve vector for article {}", article_id);
    Err(anyhow::anyhow!("Vector not found"))
}
