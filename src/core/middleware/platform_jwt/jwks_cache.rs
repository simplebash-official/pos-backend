use jsonwebtoken::DecodingKey;
use jsonwebtoken::jwk::JwkSet;

use std::collections::HashMap;

use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// How long fetched keys are trusted before a background-on-demand refetch.
const DEFAULT_TTL: Duration = Duration::from_secs(600);

/// Minimum gap between JWKS fetches, so a flood of tokens with random `kid`s
/// cannot turn this service into a request amplifier against the identity host.
const DEFAULT_MIN_REFRESH: Duration = Duration::from_secs(10);

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub(super) struct CacheState {
    pub(super) keys: HashMap<String, DecodingKey>,
    pub(super) fetched_at: Option<Instant>,
    pub(super) last_attempt: Option<Instant>,
}

/// In-memory copy of the identity service's JWKS.
#[derive(Debug)]
pub struct JwksCache {
    pub(super) url: String,
    pub(super) ttl: Duration,
    pub(super) min_refresh: Duration,
    pub(super) http: reqwest::Client,
    pub(super) state: RwLock<CacheState>,
    // Serialises refreshes so concurrent requests with an unknown `kid` cause
    // one fetch, not one each.
    pub(super) refresh_lock: tokio::sync::Mutex<()>,
}

impl JwksCache {
    pub fn new(url: impl Into<String>) -> Self {
        Self::with_intervals(url, DEFAULT_TTL, DEFAULT_MIN_REFRESH)
    }

    pub fn with_intervals(url: impl Into<String>, ttl: Duration, min_refresh: Duration) -> Self {
        Self {
            url: url.into(),
            ttl,
            min_refresh,
            http: reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .build()
                .unwrap_or_default(),
            state: RwLock::new(CacheState::default()),
            refresh_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// The key for `kid`, only while the cached JWKS is within its TTL.
    pub(super) async fn fresh_key(&self, kid: &str) -> Option<DecodingKey> {
        let state = self.state.read().await;
        let fresh = state.fetched_at.is_some_and(|at| at.elapsed() < self.ttl);
        if fresh {
            state.keys.get(kid).cloned()
        } else {
            None
        }
    }

    /// The verification key for `kid`, refreshing the JWKS when it is stale or
    /// the `kid` is unknown (subject to the refresh rate limit).
    pub(super) async fn key(&self, kid: &str) -> Option<DecodingKey> {
        if let Some(key) = self.fresh_key(kid).await {
            return Some(key);
        }

        let _guard = self.refresh_lock.lock().await;
        // Another request may have refreshed while we waited.
        if let Some(key) = self.fresh_key(kid).await {
            return Some(key);
        }
        let may_refresh = {
            let state = self.state.read().await;
            state
                .last_attempt
                .is_none_or(|at| at.elapsed() >= self.min_refresh)
        };
        if may_refresh {
            self.refresh().await;
        }
        self.state.read().await.keys.get(kid).cloned()
    }

    /// Fetches and installs the JWKS. On any failure the previous keys are kept
    /// (a brief identity outage must not lock every signed-in user out).
    pub(super) async fn refresh(&self) {
        self.state.write().await.last_attempt = Some(Instant::now());
        let fetched = async {
            let response = self.http.get(&self.url).send().await?.error_for_status()?;
            response.json::<JwkSet>().await
        }
        .await;
        match fetched {
            Ok(set) => {
                let mut keys = HashMap::new();
                for jwk in &set.keys {
                    if let (Some(kid), Ok(key)) =
                        (jwk.common.key_id.clone(), DecodingKey::from_jwk(jwk))
                    {
                        keys.insert(kid, key);
                    }
                }
                let mut state = self.state.write().await;
                state.keys = keys;
                state.fetched_at = Some(Instant::now());
            }
            Err(err) => {
                tracing::warn!(url = %self.url, error = %err, "failed to refresh identity JWKS");
            }
        }
    }
}
