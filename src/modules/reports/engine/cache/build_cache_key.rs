/// Helper to build standardized cache keys.
#[must_use]
pub fn build_cache_key(prefix: &str, params: &str) -> String {
    format!("reports:{prefix}:{params}")
}
