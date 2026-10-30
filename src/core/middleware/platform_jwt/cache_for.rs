use super::jwks_cache::JwksCache;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

static CACHES: LazyLock<Mutex<HashMap<String, Arc<JwksCache>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Process-wide cache for a JWKS URL, created on first use. Keeping it outside
/// `AppState` means the state struct (built by hand in many tests) does not
/// change when platform tokens are enabled.
pub fn cache_for(url: &str) -> Arc<JwksCache> {
    let mut caches = CACHES.lock().expect("JWKS cache registry mutex poisoned");
    caches
        .entry(url.to_string())
        .or_insert_with(|| Arc::new(JwksCache::new(url)))
        .clone()
}
