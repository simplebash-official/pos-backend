use crate::core::constants::roles;
use crate::core::error::{AppError, AppResult};
use crate::domain::users::Role;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

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
    /// A cache for the JWKS at `url` with the default lifetime and refresh limit.
    pub fn new(url: impl Into<String>) -> Self {
        Self::with_intervals(url, DEFAULT_TTL, DEFAULT_MIN_REFRESH)
    }

    /// A cache that trusts fetched keys for `ttl` and refreshes at most once per `min_refresh`.
    pub fn with_intervals(url: impl Into<String>, ttl: Duration, min_refresh: Duration) -> Self {
        Self {
            url: url.into(),
            ttl,
            min_refresh,
            http: reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .build()
                .expect("the JWKS HTTP client configuration is valid"),
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

/// What a verified platform token grants inside this backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformIdentity {
    pub sub: String,
    pub tid: Option<String>,
    pub role: Role,
    pub permissions: Vec<String>,
    /// Raw space-separated scope claim (kept so routes can require `device`).
    pub scope: String,
    /// Device id (`did`) for device tokens.
    pub device_id: Option<String>,
    /// Profile of the signed-in account, when the token carries it.
    pub email: Option<String>,
    pub name: Option<String>,
}

/// Maps token scopes onto the local role model. Platform tokens carry no
/// `role`/`permissions` claims, so this is the single place that decides what
/// a scope may do:
///   - `owner`  (the business owner's account token, e.g. on the web)  -> Admin
///   - `account` (an interactive identity login; identity only puts a `tid`
///     on it for a shop the account owns, so `verify_platform_token`
///     insists on one)                                                  -> Admin
///   - `device` (a linked desktop installation acting for its shop)     -> Admin
///   - `staff`                                                          -> Manager
///
/// The highest matching scope wins; a token with no recognised scope is
/// rejected rather than given a default role.
#[must_use]
pub fn role_for_scope(scope: &str) -> Option<Role> {
    let mut best = None;
    for s in scope.split_whitespace() {
        match s {
            "owner" | "device" | "account" => return Some(Role::Admin),
            "staff" => best = Some(Role::Manager),
            _ => {}
        }
    }
    best
}

/// Claims of a platform access token.
#[derive(Debug, Deserialize)]
pub(super) struct PlatformClaims {
    /// Account id of the caller.
    pub(super) sub: String,
    /// Tenant (business) the token is valid for.
    pub(super) tid: Option<String>,
    /// Space-separated scope values, e.g. `owner` or `device`.
    pub(super) scope: String,
    /// Registered device the token was issued to (device tokens only).
    pub(super) did: Option<String>,
    /// The account's email and display name (interactive identity tokens only).
    pub(super) email: Option<String>,
    pub(super) name: Option<String>,
}

/// Verifies an EdDSA platform token: signature against the JWKS key named by
/// the `kid` header, `exp`, and (when configured) `iss`.
pub async fn verify_platform_token(
    cache: &JwksCache,
    token: &str,
    issuer: Option<&str>,
) -> AppResult<PlatformIdentity> {
    let header = decode_header(token)?;
    if header.alg != Algorithm::EdDSA {
        return Err(AppError::unauthorized("unsupported token algorithm"));
    }
    let kid = header
        .kid
        .ok_or_else(|| AppError::unauthorized("token has no key id"))?;
    let key = cache
        .key(&kid)
        .await
        .ok_or_else(|| AppError::unauthorized("unknown token signing key"))?;

    let mut validation = Validation::new(Algorithm::EdDSA);
    if let Some(iss) = issuer {
        validation.set_issuer(&[iss]);
    }
    let claims = decode::<PlatformClaims>(token, &key, &validation)?.claims;

    let role = role_for_scope(&claims.scope)
        .ok_or_else(|| AppError::unauthorized("token scope is not permitted"))?;
    // An account session without a shop names no tenant to be Admin of.
    let only_account = claims.scope.split_whitespace().all(|s| s == "account");
    if only_account && claims.tid.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::unauthorized("account token has no shop"));
    }
    Ok(PlatformIdentity {
        sub: claims.sub,
        tid: claims.tid,
        scope: claims.scope.clone(),
        device_id: claims.did,
        email: claims.email,
        name: claims.name,
        role,
        permissions: roles::default_permissions(role)
            .iter()
            .map(|p| p.to_string())
            .collect(),
    })
}
