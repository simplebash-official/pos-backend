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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Helper to build standardized cache keys.
pub fn build_cache_key(prefix: &str, params: &str) -> String {
    format!("reports:{prefix}:{params}")
}

/// Tenant-aware cache/singleflight key. `None` (single-shop) yields exactly
/// `build_cache_key`; a tenant scope is part of the key so two tenants asking
/// for the same report never share a cached value or a coalesced query.
pub fn build_scoped_cache_key(scope: Option<&str>, prefix: &str, params: &str) -> String {
    match scope {
        None => build_cache_key(prefix, params),
        Some(tenant) => format!("reports:tenant={tenant}:{prefix}:{params}"),
    }
}

/// Tag attached to every entry of a tenant, used to scope invalidation.
pub fn tenant_tag(tenant: &str) -> String {
    format!("tenant:{tenant}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn single_shop_key_is_unchanged() {
        assert_eq!(
            build_scoped_cache_key(None, "feed", "a:b"),
            build_cache_key("feed", "a:b")
        );
    }

    #[test]
    fn tenants_get_distinct_keys_for_identical_reports() {
        let a = build_scoped_cache_key(Some("shop_a"), "feed", "overview:today");
        let b = build_scoped_cache_key(Some("shop_b"), "feed", "overview:today");
        assert_ne!(a, b);
        assert_ne!(a, build_scoped_cache_key(None, "feed", "overview:today"));
    }

    #[test]
    fn invalidate_matching_only_evicts_the_callers_tenant() {
        let cache = ReportsCache::new();
        let ttl = Duration::seconds(60);
        for t in ["shop_a", "shop_b"] {
            cache.set(
                build_scoped_cache_key(Some(t), "feed", "x"),
                json!(t),
                ttl,
                vec!["active".into(), tenant_tag(t)],
            );
        }
        assert_eq!(
            cache.invalidate_matching(&["active", &tenant_tag("shop_a")]),
            1
        );
        assert!(
            cache
                .get(&build_scoped_cache_key(Some("shop_a"), "feed", "x"))
                .is_none()
        );
        assert!(
            cache
                .get(&build_scoped_cache_key(Some("shop_b"), "feed", "x"))
                .is_some()
        );
    }
}
