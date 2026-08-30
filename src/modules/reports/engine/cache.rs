// High-performance in-memory cache for the Analytics & Reports Engine.
// Provides sub-microsecond lookups, atomic TTL expiration, and tag-based invalidation.

use std::{collections::HashMap, sync::RwLock};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub value: Value,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ReportsCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

impl ReportsCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Retrieve a cached value if present and not expired.
    pub fn get(&self, key: &str) -> Option<(Value, DateTime<Utc>, i64)> {
        let now = Utc::now();
        let read = self.entries.read().ok()?;
        if let Some(entry) = read.get(key)
            && entry.expires_at > now
        {
            let ttl_remaining_secs = (entry.expires_at - now).num_seconds();
            return Some((entry.value.clone(), entry.cached_at, ttl_remaining_secs));
        }
        None
    }

    /// Store a computed value in the cache with a specified TTL and invalidation tags.
    pub fn set(&self, key: String, value: Value, ttl: Duration, tags: Vec<String>) {
        let now = Utc::now();
        let entry = CacheEntry {
            value,
            cached_at: now,
            expires_at: now + ttl,
            tags,
        };

        if let Ok(mut write) = self.entries.write() {
            // Prune expired entries periodically if map grows large (> 2,000 keys)
            if write.len() > 2_000 {
                write.retain(|_, v| v.expires_at > now);
            }
            write.insert(key, entry);
        }
    }

    /// Invalidate all entries tagged with a specific tag (e.g. "active", "invoices", "repairs").
    pub fn invalidate_tag(&self, target_tag: &str) -> usize {
        let mut count = 0;
        if let Ok(mut write) = self.entries.write() {
            write.retain(|_, v| {
                if v.tags.iter().any(|t| t == target_tag) {
                    count += 1;
                    false
                } else {
                    true
                }
            });
        }
        count
    }

    /// Invalidate all cached entries.
    pub fn invalidate_all(&self) -> usize {
        if let Ok(mut write) = self.entries.write() {
            let count = write.len();
            write.clear();
            count
        } else {
            0
        }
    }

    /// Number of active cache entries.
    pub fn len(&self) -> usize {
        self.entries.read().map(|r| r.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Helper to build standardized cache keys.
pub fn build_cache_key(prefix: &str, params: &str) -> String {
    format!("reports:{prefix}:{params}")
}
