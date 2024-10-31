// vector_db.rs
use qdrant_client::{prelude::*, qdrant::CreateCollection};
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;

static CLIENT: OnceCell<QdrantClient> = OnceCell::const_new();

/// Initializes a Qdrant client with the provided URL and API key.
pub async fn initialize_client(
    url: &str,
    api_key: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = QdrantClient::new(QdrantClientConfig::from_url(url).with_api_key(api_key))?;
    CLIENT.set(client).expect("Client is already initialized");
    Ok(())
}

/// Adds a new article vector to the collection.
pub async fn add_article_vector(
    collection: &str,
    article_id: &str,
    vector: Vec<f32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = CLIENT.get().expect("Client not initialized");
    client
        .upsert_points(
            collection,
            vec![PointStruct::new(article_id.to_string(), vector)],
        )
        .await?;
    Ok(())
}

/// Searches for similar articles within the specified number of days.
pub async fn search_similar_articles(
    collection: &str,
    vector: Vec<f32>,
    days: i64,
    max_results: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = CLIENT.get().expect("Client not initialized");
    let search_result = client
        .search_points(collection, vector, max_results)
        .await?;
    Ok(search_result
        .result
        .iter()
        .map(|hit| hit.id.to_string())
        .collect())
}
