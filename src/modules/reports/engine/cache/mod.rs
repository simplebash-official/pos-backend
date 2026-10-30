//! High-performance in-memory cache for the Analytics & Reports Engine.
//! Provides sub-microsecond lookups, atomic TTL expiration, and tag-based invalidation.

mod engine_cache;
#[cfg(test)]
mod tests;

pub use engine_cache::CacheEntry;
pub use engine_cache::CachedValue;
pub use engine_cache::ReportsCache;
pub use engine_cache::build_cache_key;
pub use engine_cache::build_scoped_cache_key;
pub use engine_cache::tenant_tag;
