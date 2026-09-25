// Verification of platform-issued access tokens (EdDSA / Ed25519 JWTs minted by
// the cloud identity service) for a multi-tenant deployment. The identity
// service publishes its public keys as a JWKS document; `JwksCache` keeps them
// in memory, refreshes on a TTL, and refreshes early (rate limited) when a
// token names a `kid` it has not seen, which is how key rotation is picked up
// without a restart.
//
// Local HS256 tokens are untouched: `core::middleware::auth::verify_bearer`
// dispatches on the token's `alg` header and only EdDSA tokens come through
// here.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{
    core::{
        constants::roles,
        error::{AppError, AppResult},
    },
    domain::users::Role,
};

/// How long fetched keys are trusted before a background-on-demand refetch.
const DEFAULT_TTL: Duration = Duration::from_secs(600);
/// Minimum gap between JWKS fetches, so a flood of tokens with random `kid`s
/// cannot turn this service into a request amplifier against the identity host.
const DEFAULT_MIN_REFRESH: Duration = Duration::from_secs(10);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Claims of a platform access token.
#[derive(Debug, Deserialize)]
struct PlatformClaims {
    /// Account id of the caller.
    sub: String,
    /// Tenant (business) the token is valid for.
    #[serde(default)]
    tid: Option<String>,
    /// Space-separated scope values, e.g. `owner` or `device`.
    #[serde(default)]
    scope: String,
    /// Registered device the token was issued to (device tokens only).
    #[serde(default)]
    did: Option<String>,
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
}

/// Maps token scopes onto the local role model. Platform tokens carry no
/// `role`/`permissions` claims, so this is the single place that decides what
/// a scope may do:
///   - `owner`  (the business owner's account token, e.g. on the web)  -> Admin
///   - `device` (a linked desktop installation acting for its shop)     -> Admin
///   - `staff`                                                          -> Manager
///
/// The highest matching scope wins; a token with no recognised scope is
/// rejected rather than given a default role.
pub fn role_for_scope(scope: &str) -> Option<Role> {
    let mut best = None;
    for s in scope.split_whitespace() {
        match s {
            "owner" | "device" => return Some(Role::Admin),
            "staff" => best = Some(Role::Manager),
            _ => {}
        }
    }
    best
}

#[derive(Default)]
struct CacheState {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
    last_attempt: Option<Instant>,
}

/// In-memory copy of the identity service's JWKS.
pub struct JwksCache {
    url: String,
    ttl: Duration,
    min_refresh: Duration,
    http: reqwest::Client,
    state: RwLock<CacheState>,
    // Serialises refreshes so concurrent requests with an unknown `kid` cause
    // one fetch, not one each.
    refresh_lock: tokio::sync::Mutex<()>,
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

    async fn cached(&self, kid: &str) -> (Option<DecodingKey>, bool) {
        let state = self.state.read().await;
        let fresh = state
            .fetched_at
            .is_some_and(|at| at.elapsed() < self.ttl);
        (state.keys.get(kid).cloned(), fresh)
    }

    /// The verification key for `kid`, refreshing the JWKS when it is stale or
    /// the `kid` is unknown (subject to the refresh rate limit).
    async fn key(&self, kid: &str) -> Option<DecodingKey> {
        if let (Some(key), true) = self.cached(kid).await {
            return Some(key);
        }

        let _guard = self.refresh_lock.lock().await;
        // Another request may have refreshed while we waited.
        if let (Some(key), true) = self.cached(kid).await {
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
    async fn refresh(&self) {
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

static CACHES: LazyLock<Mutex<HashMap<String, Arc<JwksCache>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Process-wide cache for a JWKS URL, created on first use. Keeping it outside
/// `AppState` means the state struct (built by hand in many tests) does not
/// change when platform tokens are enabled.
pub fn cache_for(url: &str) -> Arc<JwksCache> {
    let mut caches = CACHES.lock().unwrap_or_else(|e| e.into_inner());
    caches
        .entry(url.to_string())
        .or_insert_with(|| Arc::new(JwksCache::new(url)))
        .clone()
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
    Ok(PlatformIdentity {
        sub: claims.sub,
        tid: claims.tid,
        scope: claims.scope.clone(),
        device_id: claims.did,
        role,
        permissions: roles::default_permissions(role)
            .iter()
            .map(|p| p.to_string())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    // Throwaway Ed25519 keys generated for these tests only.
    const KEY1_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIEcLbCnoF7iAMQ0miXYVh1HQdnM0SLhOA9PD/m8DntbU\n-----END PRIVATE KEY-----\n";
    const KEY1_X: &str = "9imXnTw41MUXGu8nXCsJLgXbcwkndvFwlg_9LQnyiFA";
    const KEY2_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEICj3QZKjKM8xipZdSflkxurIwCazw5cz9memkcj553zL\n-----END PRIVATE KEY-----\n";
    const KEY2_X: &str = "x0C38d6nTubgo31Rnj7WiJw1mQ-nJOEn3e-fuDMo91s";
    const KEY3_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIDoCj7Xo4YYF1o7nVfYqYxnezMKoYkc/2LkzLAp3OFZJ\n-----END PRIVATE KEY-----\n";

    const ISSUER: &str = "https://id.example.test";

    /// Serves `body` (swappable) as the JWKS on a random local port and counts fetches.
    struct MockJwks {
        url: String,
        body: Arc<Mutex<String>>,
        hits: Arc<AtomicUsize>,
    }

    async fn mock_jwks(initial: serde_json::Value) -> MockJwks {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/.well-known/jwks.json", listener.local_addr().unwrap());
        let body = Arc::new(Mutex::new(initial.to_string()));
        let hits = Arc::new(AtomicUsize::new(0));
        let (b, h) = (body.clone(), hits.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { break };
                let (b, h) = (b.clone(), h.clone());
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    h.fetch_add(1, Ordering::SeqCst);
                    let payload = b.lock().unwrap().clone();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        MockJwks { url, body, hits }
    }

    fn jwks(keys: &[(&str, &str)]) -> serde_json::Value {
        json!({ "keys": keys.iter().map(|(kid, x)| json!({
            "kty": "OKP", "crv": "Ed25519", "alg": "EdDSA", "use": "sig", "kid": kid, "x": x
        })).collect::<Vec<_>>() })
    }

    fn token(pem: &str, kid: &str, claims: serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(kid.to_string());
        encode(&header, &claims, &EncodingKey::from_ed_pem(pem.as_bytes()).unwrap()).unwrap()
    }

    fn claims(scope: &str, exp_offset: i64, iss: &str) -> serde_json::Value {
        json!({
            "sub": "acct_1", "tid": "tnt_1", "scope": scope, "iss": iss,
            "exp": chrono::Utc::now().timestamp() + exp_offset,
        })
    }

    fn cache(mock: &MockJwks, min_refresh: Duration) -> JwksCache {
        JwksCache::with_intervals(&mock.url, Duration::from_secs(600), min_refresh)
    }

    #[tokio::test]
    async fn valid_owner_token_maps_to_admin_with_tenant() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
        let id = verify_platform_token(&c, &t, Some(ISSUER)).await.unwrap();
        assert_eq!(id.sub, "acct_1");
        assert_eq!(id.tid.as_deref(), Some("tnt_1"));
        assert_eq!(id.role, Role::Admin);
        assert_eq!(
            id.permissions,
            roles::default_permissions(Role::Admin)
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn scope_mapping_is_deliberate() {
        assert_eq!(role_for_scope("device"), Some(Role::Admin));
        assert_eq!(role_for_scope("staff"), Some(Role::Manager));
        assert_eq!(role_for_scope("staff owner"), Some(Role::Admin));
        assert_eq!(role_for_scope("read"), None);
        assert_eq!(role_for_scope(""), None);

        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t = token(KEY1_PEM, "k1", claims("staff", 300, ISSUER));
        assert_eq!(
            verify_platform_token(&c, &t, Some(ISSUER)).await.unwrap().role,
            Role::Manager
        );
        let none = token(KEY1_PEM, "k1", claims("read", 300, ISSUER));
        assert!(verify_platform_token(&c, &none, Some(ISSUER)).await.is_err());
    }

    #[tokio::test]
    async fn unknown_kid_triggers_a_refresh_that_picks_up_a_rotated_key() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t1 = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
        verify_platform_token(&c, &t1, Some(ISSUER)).await.unwrap();
        let hits_after_first = mock.hits.load(Ordering::SeqCst);

        // Identity rotates: k2 is published, k1 retired.
        *mock.body.lock().unwrap() = jwks(&[("k2", KEY2_X)]).to_string();
        let t2 = token(KEY2_PEM, "k2", claims("owner", 300, ISSUER));
        verify_platform_token(&c, &t2, Some(ISSUER)).await.unwrap();
        assert!(mock.hits.load(Ordering::SeqCst) > hits_after_first);
    }

    #[tokio::test]
    async fn unknown_kid_refresh_is_rate_limited() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::from_secs(60));
        let bogus = token(KEY2_PEM, "nope", claims("owner", 300, ISSUER));
        for _ in 0..5 {
            assert!(verify_platform_token(&c, &bogus, Some(ISSUER)).await.is_err());
        }
        assert_eq!(mock.hits.load(Ordering::SeqCst), 1, "one fetch, not one per token");
    }

    #[tokio::test]
    async fn expired_token_is_rejected() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t = token(KEY1_PEM, "k1", claims("owner", -3600, ISSUER));
        assert!(verify_platform_token(&c, &t, Some(ISSUER)).await.is_err());
    }

    #[tokio::test]
    async fn wrong_issuer_is_rejected() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t = token(KEY1_PEM, "k1", claims("owner", 300, "https://evil.example"));
        assert!(verify_platform_token(&c, &t, Some(ISSUER)).await.is_err());
    }

    #[tokio::test]
    async fn tampered_or_wrongly_signed_token_is_rejected() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);

        // Signed by a different key but claiming kid k1.
        let forged = token(KEY3_PEM, "k1", claims("owner", 300, ISSUER));
        assert!(verify_platform_token(&c, &forged, Some(ISSUER)).await.is_err());

        // Payload swapped after signing (owner -> different tenant).
        let good = token(KEY1_PEM, "k1", claims("staff", 300, ISSUER));
        let other = token(KEY1_PEM, "k1", json!({
            "sub": "acct_1", "tid": "tnt_OTHER", "scope": "owner", "iss": ISSUER,
            "exp": chrono::Utc::now().timestamp() + 300,
        }));
        let mut parts: Vec<&str> = good.split('.').collect();
        let other_parts: Vec<&str> = other.split('.').collect();
        parts[1] = other_parts[1];
        let tampered = parts.join(".");
        assert!(verify_platform_token(&c, &tampered, Some(ISSUER)).await.is_err());
    }

    #[tokio::test]
    async fn non_eddsa_and_kidless_tokens_are_rejected() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let hs = encode(
            &Header::default(),
            &claims("owner", 300, ISSUER),
            &EncodingKey::from_secret(b"0123456789abcdef0123456789abcdef"),
        )
        .unwrap();
        assert!(verify_platform_token(&c, &hs, Some(ISSUER)).await.is_err());

        let no_kid = encode(
            &Header::new(Algorithm::EdDSA),
            &claims("owner", 300, ISSUER),
            &EncodingKey::from_ed_pem(KEY1_PEM.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(verify_platform_token(&c, &no_kid, Some(ISSUER)).await.is_err());
    }

    fn test_config(jwks_url: Option<String>, tenant_mode: crate::core::config::TenantMode) -> crate::core::config::Config {
        crate::core::config::Config {
            database_type: crate::core::config::DatabaseType::Sqlite,
            database_url: String::new(),
            mongodb_uri: String::new(),
            mongodb_db_name: String::new(),
            jwt_secret: "unit-test-jwt-secret-that-is-long-enough-0123".to_string(),
            port: 0,
            bind_addr: "127.0.0.1".to_string(),
            jwt_expiry_hours: 1,
            document_server_url: String::new(),
            document_server_api_key: String::new(),
            generated_documents_dir: String::new(),
            return_window_days: 30,
            auto_seed: false,
            tenant_mode,
            cors_allowed_origins: Vec::new(),
            identity_jwks_url: jwks_url,
            identity_issuer: Some(ISSUER.to_string()),
            provision_secret: None,
            app_env: "test".to_string(),
        }
    }

    fn bearer(token: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        h
    }

    fn hs256(secret: &str, claims: serde_json::Value) -> String {
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn verify_bearer_derives_the_tenant_for_both_token_kinds() {
        use crate::core::{config::TenantMode, middleware::auth::verify_bearer, tenancy::Tenant};
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let config = test_config(Some(mock.url.clone()), TenantMode::Multi);
        let exp = chrono::Utc::now().timestamp() + 300;

        // Local HS256 token carrying a tenant.
        let local = hs256(&config.jwt_secret, json!({ "sub": "u1", "exp": exp, "tid": "tnt_local" }));
        let v = verify_bearer(&bearer(&local), &config).await.unwrap();
        assert_eq!(v.tenant, Some(Tenant::id("tnt_local").unwrap()));
        assert_eq!(v.tenant_id().as_deref(), Some("tnt_local"));

        // Local token without a tenant (or an empty one): no tenant, which is
        // what makes CurrentUser reject it in multi mode.
        for claims in [json!({ "sub": "u1", "exp": exp }), json!({ "sub": "u1", "exp": exp, "tid": "  " })] {
            let t = hs256(&config.jwt_secret, claims);
            let v = verify_bearer(&bearer(&t), &config).await.unwrap();
            assert!(v.tenant.is_none() && v.tenant_id().is_none());
        }

        // Platform token: tenant, Admin role and the Admin permission set.
        let platform = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
        let v = verify_bearer(&bearer(&platform), &config).await.unwrap();
        assert_eq!(v.user_id, "acct_1");
        assert_eq!(v.tenant_id().as_deref(), Some("tnt_1"));
        assert_eq!(v.role, Some(Role::Admin));
        assert!(!v.permissions.is_empty());

        // Wrong HS256 secret still fails.
        let bad = hs256("some-other-secret-some-other-secret-0000", json!({ "sub": "u1", "exp": exp, "tid": "t" }));
        assert!(verify_bearer(&bearer(&bad), &config).await.is_err());
    }

    #[tokio::test]
    async fn platform_tokens_are_rejected_when_not_enabled() {
        use crate::core::{config::TenantMode, middleware::auth::verify_bearer};
        let config = test_config(None, TenantMode::Multi);
        let platform = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
        assert!(verify_bearer(&bearer(&platform), &config).await.is_err());
        assert!(verify_bearer(&axum::http::HeaderMap::new(), &config).await.is_err());
    }

    #[tokio::test]
    async fn jwks_outage_keeps_previously_fetched_keys() {
        let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
        let c = cache(&mock, Duration::ZERO);
        let t = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
        verify_platform_token(&c, &t, Some(ISSUER)).await.unwrap();
        // Point the cache at nothing: refresh fails, cached key must still verify.
        let dead = JwksCache::with_intervals(
            "http://127.0.0.1:1/jwks.json",
            Duration::from_secs(600),
            Duration::ZERO,
        );
        {
            let mut s = dead.state.write().await;
            s.keys = c.state.read().await.keys.clone();
            s.fetched_at = Some(Instant::now());
        }
        assert!(verify_platform_token(&dead, &t, Some(ISSUER)).await.is_ok());
    }
}
