use crate::{db::Database, LLMClient};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ProcessItemParams<'a> {
    pub topics: &'a [String],
    pub llm_client: &'a LLMClient,
    pub model: &'a str,
    pub temperature: f32,
    pub db: &'a Database,
    pub slack_token: &'a str,
    pub slack_channel: &'a str,
    pub places: Option<Value>,
}
