// High-performance in-memory cache for the Analytics & Reports Engine.
// Provides sub-microsecond lookups, atomic TTL expiration, and tag-based invalidation.

mod build_cache_key;
mod build_scoped_cache_key;
mod cache_entry;
mod cached_value;
mod reports_cache;
mod tenant_tag;
#[cfg(test)]
mod tests;

pub use build_cache_key::build_cache_key;
pub use build_scoped_cache_key::build_scoped_cache_key;
pub use cache_entry::CacheEntry;
pub use cached_value::CachedValue;
pub use reports_cache::ReportsCache;
pub use tenant_tag::tenant_tag;
