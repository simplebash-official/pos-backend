use super::build_cache_key::build_cache_key;

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
