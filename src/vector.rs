use anyhow::Result;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{
    BertModel, Config as BertConfig, HiddenAct, PositionEmbeddingType,
};
use once_cell::sync::OnceCell;
use qdrant_client::qdrant::point_id::PointIdOptions;
use qdrant_client::qdrant::vectors::VectorsOptions;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{
    CreateCollection, Distance, PointId, PointStruct, SearchParams, SearchPoints, UpsertPoints,
    VectorParams, VectorsConfig, WithPayloadSelector, WithVectorsSelector, WriteOrdering,
};
use qdrant_client::Qdrant;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokenizers::Tokenizer;
use tokio::fs;
use tokio::time::Instant;
use tracing::{error, info, warn};

// Static globals
static MODEL: OnceCell<Arc<BertModel>> = OnceCell::new();
static TOKENIZER: OnceCell<Arc<Tokenizer>> = OnceCell::new();

const MODEL_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/model.safetensors";
const TOKENIZER_URL: &str =
    "https://huggingface.co/intfloat/e5-large-v2/resolve/main/tokenizer.json";
const TARGET_VECTOR: &str = "vector";
const QDRANT_URL_ENV: &str = "QDRANT_URL";
// Number of days to look back for similar articles
const SIMILARITY_TIME_WINDOW_DAYS: i64 = 14;

/// Calculates the date threshold for similarity searches
///
/// # Returns
/// - RFC3339 formatted date string for the threshold (N days ago)
fn calculate_similarity_date_threshold() -> String {
    chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(SIMILARITY_TIME_WINDOW_DAYS))
        .unwrap_or_else(|| chrono::Utc::now())
        .to_rfc3339()
}

/**
* Created Qdrant schema:
* $ curl -X PUT 'http://qdrant:6333/collections/articles' \
*     -H 'Content-Type: application/json' \
*     -d '{
*     "name": "articles",
*     "vectors": {
*         "size": 1024,
*         "distance": "Cosine"
*     },
*     "payload_schema": {
*         "sqlite_id": "integer",
*         "published_date": {
*             "type": "datetime"
*         },
*         "category": {
*             "type": "keyword"
*         }
*     },
*     "optimizers_config": {
*         "indexing_threshold": 20000
*     },
*     "hnsw_config": {
*         "m": 16,
*         "ef_construct": 100,
*         "full_scan_threshold": 10000
*     },
*     "points_count": 0,
*     "on_disk_payload": true,
*     "points_id_type": "numeric"
* }'
*/

struct E5Config {
    model_path: String,
    tokenizer_path: String,
    dimensions: usize,
    max_length: usize,
    _similarity_threshold: f32,
    device: Device,
}

impl Default for E5Config {
    fn default() -> Self {
        Self {
            model_path: "models/e5-large-v2.safetensors".to_string(),
            tokenizer_path: "models/e5-tokenizer.json".to_string(),
            dimensions: 1024,
            max_length: 512,
            _similarity_threshold: 0.85,
            device: Device::Cpu,
        }
    }
}

impl E5Config {
    async fn ensure_models_exist(&self) -> Result<()> {
        // Create models directory if it doesn't exist
        if !Path::new("models").exists() {
            fs::create_dir("models").await?;
        }

        // Check and download model file if needed
        if !Path::new(&self.model_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 model from {}", MODEL_URL);
            let response = reqwest::get(MODEL_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.model_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 model to {}", self.model_path);
        }

        // Check and download tokenizer file if needed
        if !Path::new(&self.tokenizer_path).exists() {
            info!(target: TARGET_VECTOR, "Downloading E5 tokenizer from {}", TOKENIZER_URL);
            let response = reqwest::get(TOKENIZER_URL).await?;
            let bytes = response.bytes().await?;
            fs::write(&self.tokenizer_path, bytes).await?;
            info!(target: TARGET_VECTOR, "Downloaded E5 tokenizer to {}", self.tokenizer_path);
        }

        Ok(())
    }
}

fn init_e5_model(config: &E5Config) -> Result<()> {
    info!(target: TARGET_VECTOR, "Starting to load E5 model from {}", config.model_path);
    let bert_config = BertConfig {
        hidden_size: config.dimensions,
        intermediate_size: 4096,
        max_position_embeddings: config.max_length,
        num_attention_heads: 16,
        num_hidden_layers: 24,
        vocab_size: 30522,
        layer_norm_eps: 1e-12,
        pad_token_id: 0,
        hidden_act: HiddenAct::Gelu,
        hidden_dropout_prob: 0.0,
        type_vocab_size: 2,
        initializer_range: 0.02,
        position_embedding_type: PositionEmbeddingType::Absolute,
        use_cache: false,
        classifier_dropout: None,
        model_type: None,
    };

    // Load the safetensors file
    let tensors = match candle_core::safetensors::load_buffer(
        &std::fs::read(&config.model_path)?,
        &config.device,
    ) {
        Ok(t) => t,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load model tensors: {}", e);
            return Err(anyhow::anyhow!("Failed to load model tensors"));
        }
    };

    // Create VarBuilder from the loaded tensors
    let vb = VarBuilder::from_tensors(tensors, DType::F32, &config.device);

    // Load the model
    let model = match BertModel::load(vb, &bert_config) {
        Ok(m) => m,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load BERT model: {}", e);
            return Err(anyhow::anyhow!("Failed to load BERT model"));
        }
    };

    // Set the model in the static
    if MODEL.set(Arc::new(model)).is_err() {
        error!(target: TARGET_VECTOR, "!!! Failed to set model in static");
        return Err(anyhow::anyhow!("Failed to set model in static"));
    }

    info!(target: TARGET_VECTOR, "Successfully loaded E5 model");
    Ok(())
}

fn init_e5_tokenizer(config: &E5Config) -> Result<()> {
    info!(target: TARGET_VECTOR, "Starting to load E5 tokenizer from {}", config.tokenizer_path);

    let tokenizer = match Tokenizer::from_file(&config.tokenizer_path) {
        Ok(t) => t,
        Err(e) => {
            error!(target: TARGET_VECTOR, "!!! Failed to load tokenizer: {}", e);
            return Err(anyhow::anyhow!("Failed to load tokenizer"));
        }
    };

    if TOKENIZER.set(Arc::new(tokenizer)).is_err() {
        error!(target: TARGET_VECTOR, "!!! Failed to set tokenizer in static");
        return Err(anyhow::anyhow!("Failed to set tokenizer in static"));
    }

    info!(target: TARGET_VECTOR, "Successfully loaded E5 tokenizer");
    Ok(())
}

async fn get_article_embedding(prefixed_text: &str, config: &E5Config) -> Result<Vec<f32>> {
    let start_time = Instant::now();
    let model = MODEL.get().expect("E5 model not initialized");
    let tokenizer = TOKENIZER.get().expect("E5 tokenizer not initialized");

    let tokenize_start = Instant::now();
    let encoding = tokenizer
        .encode(prefixed_text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

    // Truncate to max_length - 1 to avoid index boundary issues
    let max_len = config.max_length - 1;
    let input_ids: Vec<i64> = encoding
        .get_ids()
        .iter()
        .take(max_len)
        .map(|&x| x as i64)
        .collect();
    let attention_mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .take(max_len)
        .map(|&x| x as i64)
        .collect();

    let tokenize_end = Instant::now();
    let inference_start = Instant::now();

    let input_ids = Tensor::new(input_ids, &config.device)?;
    let attention_mask = Tensor::new(attention_mask, &config.device)?;

    let input_ids = input_ids.unsqueeze(0)?;
    let attention_mask = attention_mask.unsqueeze(0)?;

    // Get the last hidden state
    let hidden_state = model.forward(&input_ids, &attention_mask, None)?;

    info!(target: TARGET_VECTOR, "Shape of hidden_state: {:?}", hidden_state.shape());

    // Convert attention mask to float
    let attention_mask_float = attention_mask.to_dtype(DType::F32)?;
    info!(target: TARGET_VECTOR, "Shape of attention_mask_float: {:?}", attention_mask_float.shape());

    // Expand attention mask for broadcasting (match hidden_state shape)
    let attention_mask_expanded = attention_mask_float
        .unsqueeze(2)?
        .expand(hidden_state.shape())?;
    info!(target: TARGET_VECTOR, "Shape of attention_mask_expanded: {:?}", attention_mask_expanded.shape());

    // Apply attention mask (zero out padding embeddings)
    let masked_hidden = hidden_state.mul(&attention_mask_expanded)?;
    info!(target: TARGET_VECTOR, "Shape of masked_hidden: {:?}", masked_hidden.shape());

    // Sum the masked hidden states along the sequence length dimension
    let summed_hidden = masked_hidden.sum(1)?;
    info!(target: TARGET_VECTOR, "Shape of summed_hidden: {:?}", summed_hidden.shape());

    // Sum the attention mask to count the number of valid tokens
    let valid_token_counts = attention_mask_float
        .sum(1)?
        .unsqueeze(1)?
        .clamp(1.0, f32::MAX)?;
    info!(target: TARGET_VECTOR, "Shape of valid_token_counts: {:?}", valid_token_counts.shape());

    // Perform mean pooling (ensure correct shape for division)
    let valid_token_counts_expanded = valid_token_counts.expand(summed_hidden.shape())?;
    info!(target: TARGET_VECTOR, "Shape of valid_token_counts_expanded: {:?}", valid_token_counts_expanded.shape());
    let mean_pooled = summed_hidden.div(&valid_token_counts_expanded)?;

    // Normalize the vector
    let norm = mean_pooled.sqr()?.sum(1)?.sqrt()?.unsqueeze(1)?;
    let norm_expanded = norm.expand(mean_pooled.shape())?; // [1, 1024]
    let normalized = mean_pooled.div(&norm_expanded)?;

    // Get final vector
    let vector = normalized.squeeze(0)?.to_vec1::<f32>()?;

    let end_time = Instant::now();

    // Calculate statistical properties of the embedding
    let sum: f32 = vector.iter().sum();
    let mean = sum / vector.len() as f32;
    let variance: f32 =
        vector.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / vector.len() as f32;
    let std_dev = variance.sqrt();
    let max = vector.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let min = vector.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let active_dimensions = vector.iter().filter(|&&x| x > mean).count();
    let magnitude: f32 = vector.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();

    info!(target: TARGET_VECTOR,
        "Embedding generation successful: Timing: Input length: {} tokens; Tokenization time: {:?}; Inference time: {:?}; Total time: {:?} - Statistics: Dimensions: {}; Mean: {:.4}; Std Dev: {:.4}; Min: {:.4}; Max: {:.4}; Active dimensions: {}/{} ({:.1}%); Vector magnitude: {:.6}; Original text length: {} chars",
        input_ids.dims()[1],
        tokenize_end.duration_since(tokenize_start),
        end_time.duration_since(inference_start),
        end_time.duration_since(start_time),
        vector.len(),
        mean,
        std_dev,
        min,
        max,
        active_dimensions,
        vector.len(),
        (active_dimensions as f32 / vector.len() as f32) * 100.0,
        magnitude,
        prefixed_text.len()
    );

    Ok(vector)
}

pub async fn get_article_vectors(text: &str) -> Result<Option<Vec<f32>>> {
    let config = E5Config::default();
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    let total_start = Instant::now();

    if !INITIALIZED.load(Ordering::Relaxed) {
        let init_start = Instant::now();
        config.ensure_models_exist().await?;
        let model_init_start = Instant::now();
        init_e5_model(&config)?;
        let tokenizer_init_start = Instant::now();
        init_e5_tokenizer(&config)?;
        INITIALIZED.store(true, Ordering::Relaxed);

        info!(target: TARGET_VECTOR,
            "Initialization timing: Model download/check: {:?}; Model initialization: {:?}; Tokenizer initialization: {:?}; Total init time: {:?}",
            model_init_start.duration_since(init_start),
            tokenizer_init_start.duration_since(model_init_start),
            total_start.duration_since(tokenizer_init_start),
            total_start.duration_since(init_start)
        );
    }

    // Use query-focused embedding to direct the model to focus on event identification
    let prefixed_text = format!(
        "query: What is the main event described in this article? passage: {}",
        text
    );
    match get_article_embedding(&prefixed_text, &config).await {
        Ok(embedding) => {
            let validation_start = Instant::now();

            // Basic validation
            if embedding.len() != config.dimensions {
                error!(target: TARGET_VECTOR, "Unexpected embedding dimensions: got {}, expected {}", 
                    embedding.len(), config.dimensions);
                return Ok(None);
            }

            // Statistical calculations
            let stats_start = Instant::now();

            let sum: f32 = embedding.iter().sum();
            let mean = sum / embedding.len() as f32;
            let variance: f32 =
                embedding.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / embedding.len() as f32;
            let std_dev = variance.sqrt();
            let max = embedding.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            let min = embedding.iter().fold(f32::INFINITY, |a, &b| a.min(b));

            let active_dimensions = embedding.iter().filter(|&&x| x > mean).count();

            let magnitude: f32 = embedding.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();

            let end_time = Instant::now();

            info!(target: TARGET_VECTOR, "Embedding generation complete: Timing: Embedding generation: {:?}; Validation: {:?}; Statistics calculation: {:?}; Total processing time: {:?} - Statistics: Dimensions: {}; Mean: {:.4}; Std Dev: {:.4}; Min: {:.4}; Max: {:.4}; Active dimensions: {}/{} ({:.1}%); Vector magnitude: {:.6}",
                validation_start.duration_since(total_start),
                stats_start.duration_since(validation_start),
                end_time.duration_since(stats_start),
                end_time.duration_since(total_start),
                embedding.len(),
                mean,
                std_dev,
                min,
                max,
                active_dimensions,
                embedding.len(),
                (active_dimensions as f32 / embedding.len() as f32) * 100.0,
                magnitude
            );

            Ok(Some(embedding))
        }
        Err(e) => {
            let end_time = Instant::now();
            error!(target: TARGET_VECTOR,
                "Failed to generate embedding after {:?}: {:?}",
                end_time.duration_since(total_start),
                e
            );
            Ok(None)
        }
    }
}

pub async fn store_embedding(
    sqlite_id: i64,
    embedding: &Vec<f32>,
    published_date: &str,
    category: &str,
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

    // Add basic metadata
    payload.insert(
        "published_date".to_string(),
        json!(published_date).try_into().unwrap(),
    );
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

pub async fn get_similar_articles(embedding: &Vec<f32>, limit: u64) -> Result<Vec<ArticleMatch>> {
    info!(target: TARGET_VECTOR, "search_similar_articles: embedding length = {}, using similarity time window of {} days", embedding.len(), SIMILARITY_TIME_WINDOW_DAYS);

    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

    // Calculate date threshold for recent articles
    let date_threshold = calculate_similarity_date_threshold();
    info!(target: TARGET_VECTOR, "Using date threshold for similarity search: {}", date_threshold);

    // Parse the threshold date
    let parsed_date = chrono::DateTime::parse_from_rfc3339(&date_threshold).unwrap();
    let timestamp_seconds = parsed_date.timestamp();

    // Create the search request with date filter
    let search_points = SearchPoints {
        collection_name: "articles".to_string(),
        vector: embedding.clone(),
        limit,
        with_payload: Some(WithPayloadSelector::from(true)),
        with_vectors: Some(WithVectorsSelector::from(false)),
        filter: Some(qdrant_client::qdrant::Filter {
            must: vec![qdrant_client::qdrant::Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    qdrant_client::qdrant::FieldCondition {
                        key: "published_date".to_string(),
                        r#match: None,
                        range: None,
                        datetime_range: Some(qdrant_client::qdrant::DatetimeRange {
                            gt: Some(qdrant_client::qdrant::Timestamp {
                                seconds: timestamp_seconds,
                                nanos: 0,
                            }),
                            lt: None,
                            gte: None,
                            lte: None,
                        }),
                        geo_bounding_box: None,
                        geo_radius: None,
                        values_count: None,
                        geo_polygon: None,
                    },
                )),
            }],
            should: vec![],
            must_not: vec![],
            min_should: None,
        }),
        params: Some(SearchParams {
            hnsw_ef: Some(128),
            exact: Some(true),
            ..Default::default()
        }),
        score_threshold: Some(0.80),
        // The sort field doesn't exist on SearchPoints, we'll sort after fetching
        ..Default::default()
    };

    match client.search_points(search_points).await {
        Ok(response) => {
            let mut matches: Vec<ArticleMatch> = response
                .result
                .into_iter()
                .map(|scored_point| {
                    let id = match scored_point.id.unwrap().point_id_options.unwrap() {
                        PointIdOptions::Num(num) => num as i64,
                        _ => panic!("Expected numeric point ID"),
                    };

                    let payload = scored_point.payload;
                    let published_date = payload
                        .get("published_date")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::StringValue(s) = k {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();

                    let category = payload
                        .get("category")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::StringValue(s) = k {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();

                    let quality_score = payload
                        .get("quality_score")
                        .and_then(|v| v.kind.as_ref())
                        .and_then(|k| {
                            if let qdrant_client::qdrant::value::Kind::IntegerValue(i) = k {
                                Some(*i as i8)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);

                    ArticleMatch {
                        id,
                        published_date,
                        category,
                        quality_score,
                        score: scored_point.score,

                        // New fields with default values
                        vector_score: Some(scored_point.score), // Vector score is the same as score for pure vector matches
                        vector_active_dimensions: None,
                        vector_magnitude: None,
                        entity_overlap_count: None,
                        primary_overlap_count: None,
                        person_overlap: None,
                        org_overlap: None,
                        location_overlap: None,
                        event_overlap: None,
                        temporal_proximity: None,
                        similarity_formula: Some(
                            "60% vector similarity (no entity data available)".to_string(),
                        ),
                    }
                })
                .collect();

            // Sort by quality_score in descending order
            matches.sort_by(|a, b| b.quality_score.cmp(&a.quality_score));

            info!(
                target: TARGET_VECTOR,
                "Found {} similar articles", matches.len()
            );
            Ok(matches)
        }
        Err(e) => {
            error!(
                target: TARGET_VECTOR,
                "Failed to search for similar articles: {:?}", e
            );
            Err(anyhow::anyhow!(
                "Failed to search for similar articles: {:?}",
                e
            ))
        }
    }
}

#[derive(Debug)]
pub struct ArticleMatch {
    // Basic article identification and metadata
    pub id: i64,
    pub published_date: String,
    pub category: String,
    pub quality_score: i8,
    pub score: f32, // final combined similarity score

    // Vector similarity metrics
    pub vector_score: Option<f32>, // Raw vector similarity score
    pub vector_active_dimensions: Option<usize>, // Number of active dimensions in vector
    pub vector_magnitude: Option<f32>, // Vector magnitude

    // Entity similarity metrics
    pub entity_overlap_count: Option<usize>, // Total number of overlapping entities
    pub primary_overlap_count: Option<usize>, // Number of primary entities that overlap
    pub person_overlap: Option<f32>,         // Person similarity score (0-1)
    pub org_overlap: Option<f32>,            // Organization similarity score (0-1)
    pub location_overlap: Option<f32>,       // Location similarity score (0-1)
    pub event_overlap: Option<f32>,          // Event similarity score (0-1)
    pub temporal_proximity: Option<f32>,     // Temporal proximity score (0-1)

    // Formula explanation
    pub similarity_formula: Option<String>, // Explanation of how the score was calculated
}

/// Enhanced article match with both vector and entity similarity
#[derive(Debug)]
struct EnhancedArticleMatch {
    article_id: i64,
    vector_score: f32,
    entity_similarity: crate::entity::EntitySimilarityMetrics,
    final_score: f32,
    category: String,
    published_date: String,
    quality_score: i8,
}

/// Find similar articles using a dual-query approach that combines vector similarity with entity matching
pub async fn get_similar_articles_with_entities(
    embedding: &Vec<f32>,
    limit: u64,
    entity_ids: Option<&[i64]>,
    event_date: Option<&str>,
    source_article_id: Option<i64>, // For tracking the source article
) -> Result<Vec<ArticleMatch>> {
    if let Some(id) = source_article_id {
        info!(target: TARGET_VECTOR, "Starting similarity search for source article ID: {}", id);
    }

    info!(target: TARGET_VECTOR, "Starting enhanced entity-aware article search with dual-query approach");

    // Log entity IDs detail
    if let Some(ids) = entity_ids {
        info!(target: TARGET_VECTOR, "Using {} entity IDs for similar article search: {:?}", 
              ids.len(), ids);

        // Check if these entities exist in the database
        if let Some(id) = source_article_id {
            let db = crate::db::Database::instance().await;
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM article_entities WHERE article_id = ?",
            )
            .bind(id)
            .fetch_one(db.pool())
            .await
            {
                Ok(count) => {
                    info!(target: TARGET_VECTOR, "Article {} has {} entities in the database", id, count);
                    if count == 0 && ids.len() > 0 {
                        warn!(target: TARGET_VECTOR, "Database shows 0 entities for article {} but received {} entity IDs", 
                             id, ids.len());
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "Failed to check entity count for article {}: {}", id, e);
                }
            }
        }
    } else {
        info!(target: TARGET_VECTOR, "No entity IDs provided for similar article search");
    }

    let mut all_matches = std::collections::HashMap::new();

    // 1. Vector-based query using current implementation
    info!(target: TARGET_VECTOR, "Performing vector similarity search...");
    let vector_matches = get_similar_articles(embedding, limit * 2).await?; // Get more results for better coverage

    // Store the count before consuming the vector
    let vector_only_count = vector_matches.len();
    info!(target: TARGET_VECTOR, "Vector search returned {} results", vector_only_count);

    // Add vector matches to our result set
    for article in vector_matches {
        all_matches.insert(article.id, article);
    }

    // 2. Entity-based query (if we have entity IDs)
    if let Some(ids) = entity_ids {
        if !ids.is_empty() {
            info!(target: TARGET_VECTOR, "Performing entity-based search with {} entity IDs...", ids.len());
            // SET LOG LEVEL TO TRACE/DEBUG
            info!(target: TARGET_VECTOR, "CRITICAL DEBUG: About to call get_articles_by_entities with source_article_id: {:?}", source_article_id);

            match get_articles_by_entities(ids, limit * 2, source_article_id).await {
                Ok(entity_matches) => {
                    info!(target: TARGET_VECTOR, 
                        "Entity search returned {} results for entity IDs: {:?}",
                        entity_matches.len(), ids);

                    if entity_matches.is_empty() {
                        error!(target: TARGET_VECTOR, 
                            "CRITICAL: Entity search returned NO matches despite having valid entity IDs - database inconsistency possible");
                    }

                    // Continue processing with entity_matches
                    // For entity matches, calculate vector similarity if not already included
                    for mut article in entity_matches {
                        if !all_matches.contains_key(&article.id) {
                            // Calculate vector similarity for this entity match
                            match calculate_vector_similarity(embedding, article.id).await {
                                Ok(vector_score) => {
                                    info!(target: TARGET_VECTOR, 
                                        "Added entity-based match: article_id={}, entity_overlap={}, vector_score={:.4}",
                                        article.id, article.entity_overlap_count.unwrap_or(0), vector_score);
                                    article.score = vector_score; // Update with actual vector score
                                    all_matches.insert(article.id, article);
                                }
                                Err(e) => {
                                    error!(target: TARGET_VECTOR, "Failed to calculate vector similarity for article {}: {:?}", article.id, e);
                                    // Still include the article even if we couldn't get vector similarity
                                    article.score = 0.0;
                                    all_matches.insert(article.id, article);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "CRITICAL ERROR in get_articles_by_entities: {:?}", e);
                }
            }
        }
    }

    if all_matches.is_empty() {
        error!(target: TARGET_VECTOR, "CRITICAL: No matches found through either vector or entity-based search");
        return Ok(vec![]);
    }

    info!(target: TARGET_VECTOR, "Found {} total unique articles from both queries", all_matches.len());

    // Calculate entity-only count
    let entity_only_count = entity_ids.map_or(0, |ids| {
        if ids.is_empty() {
            0
        } else {
            all_matches.len() - vector_only_count
        }
    });

    info!(target: TARGET_VECTOR, 
        "Match sources: {} from vector similarity, {} from entity similarity",
        vector_only_count, entity_only_count);

    // 3. Now enhance all matches with entity similarity scores
    let mut enhanced_matches = Vec::new();
    for (id, article) in all_matches {
        // Get entity data for this article
        let article_entities = match get_article_entities(id).await {
            Ok(Some(entities)) => entities,
            Ok(None) => {
                // Still include articles without entities
                let enhanced = EnhancedArticleMatch {
                    article_id: id,
                    vector_score: article.score,
                    entity_similarity: crate::entity::EntitySimilarityMetrics::new(),
                    final_score: 0.6 * article.score, // Apply consistent 60% weighting
                    category: article.category,
                    published_date: article.published_date,
                    quality_score: article.quality_score,
                };
                enhanced_matches.push(enhanced);
                continue;
            }
            Err(e) => {
                error!(target: TARGET_VECTOR, "Failed to get entities for article {}: {:?}", id, e);
                continue;
            }
        };

        // If we have both our source entities and this article's entities, calculate similarity
        if let Some(ids) = entity_ids {
            // Create a source entities object from the IDs, passing the source article ID
            let source_entities = match build_entities_from_ids(ids).await {
                Ok(entities) => {
                    if entities.entities.is_empty() && source_article_id.is_some() {
                        // If we got no entities but have a source article ID, try to get them directly
                        match get_article_entities(source_article_id.unwrap()).await {
                            Ok(Some(direct_entities)) => {
                                info!(target: TARGET_VECTOR, "Retrieved entities directly from source article {}", source_article_id.unwrap());
                                direct_entities
                            }
                            _ => entities,
                        }
                    } else {
                        entities
                    }
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "Failed to build source entities: {:?}", e);
                    // Try direct retrieval as fallback if we have source article ID
                    if let Some(id) = source_article_id {
                        match get_article_entities(id).await {
                            Ok(Some(entities)) => {
                                info!(target: TARGET_VECTOR, "Retrieved entities directly after build failure for article {}", id);
                                entities
                            }
                            _ => crate::entity::ExtractedEntities::new(), // Empty fallback
                        }
                    } else {
                        crate::entity::ExtractedEntities::new() // Empty fallback
                    }
                }
            };

            // Calculate entity similarity between articles
            let entity_sim = crate::entity::matching::calculate_entity_similarity(
                &source_entities,
                &article_entities,
                event_date,
                Some(&article.published_date),
            );

            // Log entity similarity calculation details
            info!(target: TARGET_VECTOR,
                "Entity similarity for article {}: entity_score={:.4}, person={:.2}, org={:.2}, location={:.2}, event={:.2}, overlap_count={}",
                id, entity_sim.combined_score,
                entity_sim.person_overlap, entity_sim.organization_overlap,
                entity_sim.location_overlap, entity_sim.event_overlap,
                entity_sim.entity_overlap_count
            );

            // Create enhanced match with combined score
            let enhanced = EnhancedArticleMatch {
                article_id: id,
                vector_score: article.score,
                entity_similarity: entity_sim.clone(),
                // Combined score: 60% vector + 40% entity (as specified in activeContext.md)
                final_score: 0.6 * article.score + 0.4 * entity_sim.combined_score,
                category: article.category,
                published_date: article.published_date,
                quality_score: article.quality_score,
            };

            enhanced_matches.push(enhanced);
        } else {
            // Without source entities, apply consistent 60% weighting to vector score
            // Create a new entity metrics with defaults
            let empty_metrics = crate::entity::EntitySimilarityMetrics::new();

            let enhanced = EnhancedArticleMatch {
                article_id: id,
                vector_score: article.score,
                entity_similarity: empty_metrics,
                final_score: 0.6 * article.score, // Apply consistent 60% weighting
                category: article.category,
                published_date: article.published_date,
                quality_score: article.quality_score,
            };
            enhanced_matches.push(enhanced);
        }
    }

    // 4. Sort by final_score and apply threshold
    enhanced_matches.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    info!(target: TARGET_VECTOR, "Enhanced matches before filtering: {}", enhanced_matches.len());

    // Log all candidate matches before filtering
    for m in &enhanced_matches {
        info!(target: TARGET_VECTOR,
            "PRE-FILTER: article_id={}, vector_score={:.4}, entity_score={:.4}, final_score={:.4}, entity_overlap={}, primary_overlap={}",
            m.article_id, m.vector_score, m.entity_similarity.combined_score, m.final_score,
            m.entity_similarity.entity_overlap_count, m.entity_similarity.primary_overlap_count
        );
    }

    // Apply minimum combined threshold (0.75)
    let final_matches: Vec<ArticleMatch> = enhanced_matches
        .into_iter()
        .filter(|m| {
            let passes = m.final_score >= 0.75;
            if !passes {
                info!(target: TARGET_VECTOR,
                    "FILTERED OUT: article_id={}, final_score={:.4} (below 0.75), vector={:.4}, entity={:.4}, overlap={}",
                    m.article_id, m.final_score, m.vector_score, m.entity_similarity.combined_score,
                    m.entity_similarity.entity_overlap_count
                );
            }
            passes
        })
        .take(limit as usize)
        .map(|m| {
            info!(target: TARGET_VECTOR,
                "Match article_id={}, vector_score={:.4}, entity_score={:.4}, final_score={:.4}, primary_overlap={}",
                m.article_id, m.vector_score, m.entity_similarity.combined_score, m.final_score, m.entity_similarity.primary_overlap_count
            );

            // Create the formula string
            let formula = format!(
                "60% vector similarity ({:.2}) + 40% entity similarity ({:.2}), where entity similarity combines person (30%), organization (20%), location (15%), event (15%), and temporal (20%) factors",
                m.vector_score,
                m.entity_similarity.combined_score
            );

            ArticleMatch {
                id: m.article_id,
                published_date: m.published_date,
                category: m.category,
                quality_score: m.quality_score,
                score: m.final_score, // Use the combined score

                // Add vector metrics
                vector_score: Some(m.vector_score),
                vector_active_dimensions: None, // Not tracked for enhanced matches
                vector_magnitude: None, // Not tracked for enhanced matches

                // Add entity metrics
                entity_overlap_count: Some(m.entity_similarity.entity_overlap_count),
                primary_overlap_count: Some(m.entity_similarity.primary_overlap_count),
                person_overlap: Some(m.entity_similarity.person_overlap),
                org_overlap: Some(m.entity_similarity.organization_overlap),
                location_overlap: Some(m.entity_similarity.location_overlap),
                event_overlap: Some(m.entity_similarity.event_overlap),
                temporal_proximity: Some(m.entity_similarity.temporal_proximity),

                // Add formula explanation
                similarity_formula: Some(formula),
            }
        })
        .collect();

    info!(target: TARGET_VECTOR, "Final matched article count after filtering: {}", final_matches.len());

    // Add result verification logs to confirm all matches have entity overlap
    let with_entity_overlap = final_matches
        .iter()
        .filter(|m| m.entity_overlap_count.unwrap_or(0) > 0)
        .count();

    info!(target: TARGET_VECTOR,
        "FILTER RESULTS: Total matches: {}, With entity overlap: {}, No entity overlap: {}",
        final_matches.len(),
        with_entity_overlap,
        final_matches.len() - with_entity_overlap
    );

    if final_matches.len() - with_entity_overlap > 0 {
        // This should never happen with our fix, so log it as an error if it does
        error!(target: TARGET_VECTOR,
            "ERROR: Found {} articles without entity overlap that passed the filter threshold!",
            final_matches.len() - with_entity_overlap
        );
    }

    Ok(final_matches)
}

/// Get articles that share significant entities with the given entity IDs
async fn get_articles_by_entities(
    entity_ids: &[i64],
    limit: u64,
    source_article_id: Option<i64>,
) -> Result<Vec<ArticleMatch>> {
    let db = crate::db::Database::instance().await;

    // Get the source article's publication date if we have a source article ID
    let source_date = if let Some(id) = source_article_id {
        match db.get_article_details_with_dates(id).await {
            Ok((pub_date, _)) => pub_date,
            Err(e) => {
                error!(target: TARGET_VECTOR, "Failed to get source article date: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Use the article's own date, or current date as fallback
    let date_for_search = source_date.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    info!(target: TARGET_VECTOR, "Using source date for article similarity: {}", date_for_search);

    // Use the database function to get articles by entities within a date window
    let entity_matches = db
        .get_articles_by_entities_with_date(entity_ids, limit, &date_for_search)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get articles by entities: {}", e))?;

    // Convert to ArticleMatch objects
    let matches: Vec<ArticleMatch> = entity_matches
        .into_iter()
        .map(
            |(id, published_date, category, quality_score, primary_count, total_count)| {
                let quality_score = quality_score.unwrap_or(0) as i8;

                // Calculate a preliminary score based on entity overlap
                let primary_count = primary_count as f32;
                let total_count = total_count as f32;

                // Score formula: prioritize PRIMARY entities but also consider total overlap
                let entity_score = if total_count > 0.0 {
                    (0.7 * (primary_count / entity_ids.len() as f32))
                        + (0.3 * (total_count / entity_ids.len() as f32))
                } else {
                    0.0
                };

                ArticleMatch {
                    id,
                    published_date: published_date.unwrap_or_default(),
                    category: category.unwrap_or_default(),
                    quality_score,
                    score: entity_score, // Preliminary score, will be refined later

                    // Add entity metrics fields
                    vector_score: None, // Will be calculated later
                    vector_active_dimensions: None,
                    vector_magnitude: None,
                    entity_overlap_count: Some(total_count as usize),
                    primary_overlap_count: Some(primary_count as usize),
                    person_overlap: None, // Detailed metrics not available at this stage
                    org_overlap: None,
                    location_overlap: None,
                    event_overlap: None,
                    temporal_proximity: None,
                    similarity_formula: Some(format!("Entity-based score: 70% primary entities ({} of {}) + 30% total entities ({} of {})",
                        primary_count as usize, entity_ids.len(), total_count as usize, entity_ids.len())),
                }
            },
        )
        .collect();

    info!(target: TARGET_VECTOR, "Found {} articles by entity matching", matches.len());
    Ok(matches)
}

/// Calculate vector similarity between an embedding and a specific article
async fn calculate_vector_similarity(embedding: &Vec<f32>, article_id: i64) -> Result<f32> {
    let client = Qdrant::from_url(
        &std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required"),
    )
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

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

    // Extract the vector from the response
    if let Some(point) = response.result.first() {
        if let Some(vectors) = &point.vectors {
            if let Some(opts) = &vectors.vectors_options {
                // Extract the vector data using fully qualified type path
                let vector_data = match opts {
                    &qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(ref v) => {
                        &v.data
                    }
                    _ => {
                        error!(target: TARGET_VECTOR, "Unexpected vector format for article {}", article_id);
                        return Ok(0.0);
                    }
                };

                // Calculate cosine similarity
                let dot_product: f32 = embedding
                    .iter()
                    .zip(vector_data.iter())
                    .map(|(a, b)| a * b)
                    .sum();

                let mag1: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                let mag2: f32 = vector_data.iter().map(|x| x * x).sum::<f32>().sqrt();

                let similarity = if mag1 > 0.0 && mag2 > 0.0 {
                    dot_product / (mag1 * mag2)
                } else {
                    0.0
                };

                return Ok(similarity);
            }
        }
    }

    // If we couldn't get the vector or calculate similarity
    error!(
        target: TARGET_VECTOR,
        "Failed to calculate vector similarity for article {}", article_id
    );
    Ok(0.0) // Default to no similarity
}

/// Get all entities for a specific article
async fn get_article_entities(article_id: i64) -> Result<Option<crate::entity::ExtractedEntities>> {
    let db = crate::db::Database::instance().await;

    // Get article's date information
    let (_pub_date, event_date) = db.get_article_details_with_dates(article_id).await?;

    // Create an extracted entities object
    let mut extracted = crate::entity::ExtractedEntities::new();

    // Set event date if available
    if let Some(date) = event_date {
        extracted = extracted.with_event_date(&date);
    }

    // Get all entities linked to this article
    let entities = db.get_article_entities(article_id).await?;

    // Convert database rows to Entity objects
    for (entity_id, name, entity_type_str, importance_str) in entities {
        let entity_type = crate::entity::EntityType::from(entity_type_str.as_str());
        let importance = crate::entity::ImportanceLevel::from(importance_str.as_str());

        // Create entity and add to collection
        let entity =
            crate::entity::Entity::new(&name, &name.to_lowercase(), entity_type, importance)
                .with_id(entity_id);

        extracted.add_entity(entity);
    }

    if extracted.entities.is_empty() {
        return Ok(None);
    }

    Ok(Some(extracted))
}

/// Build an ExtractedEntities object from entity IDs
async fn build_entities_from_ids(entity_ids: &[i64]) -> Result<crate::entity::ExtractedEntities> {
    let db = crate::db::Database::instance().await;
    let mut extracted = crate::entity::ExtractedEntities::new();

    info!(target: TARGET_VECTOR, "Building source entities from {} entity IDs with detailed tracing: {:?}", entity_ids.len(), entity_ids);

    // If there are entity IDs, try to determine the article ID they're from
    // by checking the article_entities table
    if !entity_ids.is_empty() {
        let article_id_result = sqlx::query_scalar::<_, i64>(
            "SELECT article_id FROM article_entities WHERE entity_id = ? LIMIT 1",
        )
        .bind(entity_ids[0]) // Just use the first entity ID to find the article
        .fetch_optional(db.pool())
        .await;

        if let Ok(Some(article_id)) = article_id_result {
            // Found source article - get all entities with proper relationships
            info!(target: TARGET_VECTOR, "Found source article ID {} for entity ID {}", article_id, entity_ids[0]);

            match db.get_article_entities(article_id).await {
                Ok(article_entities) => {
                    info!(target: TARGET_VECTOR, "Database returned {} total entities for article {}", article_entities.len(), article_id);

                    // Filter to include only entities in our entity_ids list
                    for (entity_id, name, entity_type_str, importance_str) in article_entities {
                        if entity_ids.contains(&entity_id) {
                            let entity_type =
                                crate::entity::EntityType::from(entity_type_str.as_str());
                            let importance =
                                crate::entity::ImportanceLevel::from(importance_str.as_str());

                            let entity = crate::entity::Entity::new(
                                &name,
                                &name.to_lowercase(),
                                entity_type,
                                importance, // Use actual importance from database
                            )
                            .with_id(entity_id);

                            info!(target: TARGET_VECTOR, "Added entity with proper relationship: id={}, name='{}', type={}, importance={}", 
                                entity_id, name, entity_type_str, importance_str);
                            extracted.add_entity(entity);
                        }
                    }

                    // If we didn't match all entities, fall back to basic lookup for the rest
                    if extracted.entities.len() < entity_ids.len() {
                        let found_ids: std::collections::HashSet<i64> =
                            extracted.entities.iter().filter_map(|e| e.id).collect();

                        let missing_ids: Vec<i64> = entity_ids
                            .iter()
                            .filter(|&&id| !found_ids.contains(&id))
                            .copied()
                            .collect();

                        info!(target: TARGET_VECTOR, "Missing {} entities, falling back to basic lookup for IDs: {:?}", 
                              missing_ids.len(), missing_ids);

                        // Fall back to basic lookup for missing entities
                        for &id in &missing_ids {
                            if let Ok(Some((name, entity_type_str, _parent_id))) =
                                db.get_entity_details(id).await
                            {
                                let entity_type =
                                    crate::entity::EntityType::from(entity_type_str.as_str());

                                let entity = crate::entity::Entity::new(
                                    &name,
                                    &name.to_lowercase(),
                                    entity_type,
                                    crate::entity::ImportanceLevel::Primary, // Default to PRIMARY for fallback
                                )
                                .with_id(id);

                                info!(target: TARGET_VECTOR, "Added fallback entity: id={}, name='{}', type={}", 
                                      id, name, entity_type_str);
                                extracted.add_entity(entity);
                            } else {
                                error!(target: TARGET_VECTOR, "Failed to get details for entity ID {} - entity is missing from database", id);
                            }
                        }
                    }

                    // Add detailed entity breakdown
                    info!(
                        target: TARGET_VECTOR,
                        "Entity retrieval details: total entities={}, by importance: PRIMARY={}, SECONDARY={}, MENTIONED={}",
                        extracted.entities.len(),
                        extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Primary).count(),
                        extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Secondary).count(),
                        extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Mentioned).count()
                    );

                    // Add entity type breakdown
                    info!(
                        target: TARGET_VECTOR,
                        "Entity types breakdown: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
                        extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Person).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Organization).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Location).count(),
                        extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Event).count()
                    );

                    info!(target: TARGET_VECTOR, "Built {} entities using article relationship data", extracted.entities.len());
                    return Ok(extracted);
                }
                Err(e) => {
                    error!(target: TARGET_VECTOR, "Failed to get article entities for article {}: {}", article_id, e);
                    // Fall through to basic lookup below
                }
            }
        } else if let Err(e) = article_id_result {
            error!(target: TARGET_VECTOR, "Database error when trying to find source article for entity ID {}: {}", entity_ids[0], e);
        } else {
            warn!(target: TARGET_VECTOR, "Could not determine source article ID for entity ID {}", entity_ids[0]);
        }
    }

    // Fall back to basic entity lookup if we couldn't find relationship data
    for &id in entity_ids {
        if let Ok(Some((name, entity_type_str, _parent_id))) = db.get_entity_details(id).await {
            let entity_type = crate::entity::EntityType::from(entity_type_str.as_str());

            // Create entity with PRIMARY importance (these are our source entities)
            let entity = crate::entity::Entity::new(
                &name,
                &name.to_lowercase(),
                entity_type,
                crate::entity::ImportanceLevel::Primary,
            )
            .with_id(id);

            info!(target: TARGET_VECTOR, "Added source entity: id={}, name='{}', type={}", 
                  id, name, entity_type_str);
            extracted.add_entity(entity);
        } else {
            error!(target: TARGET_VECTOR, "Failed to get details for entity ID {} - entity is missing from database", id);
        }
    }

    // Critical error if we have no entities despite having IDs
    if extracted.entities.is_empty() && !entity_ids.is_empty() {
        error!(
            target: TARGET_VECTOR,
            "CRITICAL: Failed to retrieve any entities despite having {} entity IDs: {:?}",
            entity_ids.len(), entity_ids
        );
    }

    // Add detailed entity breakdown
    if !extracted.entities.is_empty() {
        info!(
            target: TARGET_VECTOR,
            "Entity retrieval details: total entities={}, by importance: PRIMARY={}, SECONDARY={}, MENTIONED={}",
            extracted.entities.len(),
            extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Primary).count(),
            extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Secondary).count(),
            extracted.entities.iter().filter(|e| e.importance == crate::entity::ImportanceLevel::Mentioned).count()
        );

        // Add entity type breakdown
        info!(
            target: TARGET_VECTOR,
            "Entity types breakdown: PERSON={}, ORGANIZATION={}, LOCATION={}, EVENT={}",
            extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Person).count(),
            extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Organization).count(),
            extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Location).count(),
            extracted.entities.iter().filter(|e| e.entity_type == crate::entity::EntityType::Event).count()
        );
    }

    info!(target: TARGET_VECTOR, "Built {} source entities from {} entity IDs using basic lookup", 
          extracted.entities.len(), entity_ids.len());

    Ok(extracted)
}

async fn _create_collection() -> Result<()> {
    let qdrant_url =
        std::env::var(QDRANT_URL_ENV).expect("QDRANT_URL environment variable required");
    let client = Qdrant::from_url(&qdrant_url)
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let vector_params = VectorParams {
        size: 1024, // E5 embedding size
        distance: Distance::Cosine as i32,
        ..Default::default()
    };

    let create_collection = CreateCollection {
        collection_name: "articles".to_string(),
        vectors_config: Some(VectorsConfig {
            config: Some(Config::Params(vector_params)),
        }),
        ..Default::default()
    };

    client.create_collection(create_collection).await?;

    Ok(())
}
