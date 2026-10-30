use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;

/// Helper to build standardized cache keys.
#[must_use]
pub fn build_cache_key(prefix: &str, params: &str) -> String {
    format!("reports:{prefix}:{params}")
}

/// Tenant-aware cache/singleflight key. `None` (single-shop) yields exactly
/// `build_cache_key`; a tenant scope is part of the key so two tenants asking
/// for the same report never share a cached value or a coalesced query.
#[must_use]
pub fn build_scoped_cache_key(scope: Option<&str>, prefix: &str, params: &str) -> String {
    match scope {
        None => build_cache_key(prefix, params),
        Some(tenant) => format!("reports:tenant={tenant}:{prefix}:{params}"),
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub value: Value,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/// A live cache hit: the stored value, when it was cached and how long it stays valid.
#[derive(Debug)]
pub struct CachedValue {
    pub value: Value,
    pub cached_at: DateTime<Utc>,
    pub ttl_remaining_secs: i64,
}

#[derive(Debug, Default)]
/// An in-memory cache of report results with a lifetime and invalidation tags per entry.
pub struct ReportsCache {
    pub(super) entries: RwLock<HashMap<String, CacheEntry>>,
}

impl ReportsCache {
    #[must_use]
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Retrieve a cached value if present and not expired.
    pub fn get(&self, key: &str) -> Option<CachedValue> {
        let now = Utc::now();
        let read = self.entries.read().ok()?;
        if let Some(entry) = read.get(key)
            && entry.expires_at > now
        {
            let ttl_remaining_secs = (entry.expires_at - now).num_seconds();
            return Some(CachedValue {
                value: entry.value.clone(),
                cached_at: entry.cached_at,
                ttl_remaining_secs,
            });
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

    /// Invalidate entries carrying ALL of `required` tags (e.g. `active` plus a
    /// tenant tag, so one tenant's write never evicts another tenant's cache).
    pub fn invalidate_matching(&self, required: &[&str]) -> usize {
        let mut count = 0;
        if let Ok(mut write) = self.entries.write() {
            write.retain(|_, v| {
                if required.iter().all(|r| v.tags.iter().any(|t| t == r)) {
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

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Tag attached to every entry of a tenant, used to scope invalidation.
#[must_use]
pub fn tenant_tag(tenant: &str) -> String {
    format!("tenant:{tenant}")
}
