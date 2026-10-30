use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub value: Value,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub tags: Vec<String>,
}
