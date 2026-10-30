use chrono::{DateTime, Utc};
use serde_json::Value;

/// A live cache hit: the stored value, when it was cached and how long it stays valid.
#[derive(Debug)]
pub struct CachedValue {
    pub value: Value,
    pub cached_at: DateTime<Utc>,
    pub ttl_remaining_secs: i64,
}
