use super::jwks_cache::JwksCache;
use super::role_for_scope::role_for_scope;
use super::verify_platform_token::verify_platform_token;
use crate::core::constants::roles;

use crate::domain::users::Role;

use jsonwebtoken::Algorithm;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// Throwaway Ed25519 keys generated for these tests only.
const KEY1_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIEcLbCnoF7iAMQ0miXYVh1HQdnM0SLhOA9PD/m8DntbU\n-----END PRIVATE KEY-----\n";
const KEY1_X: &str = "9imXnTw41MUXGu8nXCsJLgXbcwkndvFwlg_9LQnyiFA";
const KEY2_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEICj3QZKjKM8xipZdSflkxurIwCazw5cz9memkcj553zL\n-----END PRIVATE KEY-----\n";
const KEY2_X: &str = "x0C38d6nTubgo31Rnj7WiJw1mQ-nJOEn3e-fuDMo91s";
const KEY3_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIDoCj7Xo4YYF1o7nVfYqYxnezMKoYkc/2LkzLAp3OFZJ\n-----END PRIVATE KEY-----\n";

const ISSUER: &str = "https://id.example.test";

/// Serves `body` (swappable) as the JWKS on a random local port and counts fetches.
#[derive(Debug)]
struct MockJwks {
    url: String,
    body: Arc<Mutex<String>>,
    hits: Arc<AtomicUsize>,
}

async fn mock_jwks(initial: serde_json::Value) -> MockJwks {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/.well-known/jwks.json",
        listener.local_addr().unwrap()
    );
    let body = Arc::new(Mutex::new(initial.to_string()));
    let hits = Arc::new(AtomicUsize::new(0));
    let (b, h) = (body.clone(), hits.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
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
    encode(
        &header,
        &claims,
        &EncodingKey::from_ed_pem(pem.as_bytes()).unwrap(),
    )
    .unwrap()
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
    assert_eq!(role_for_scope("account"), Some(Role::Admin));
    assert_eq!(role_for_scope("read"), None);
    assert_eq!(role_for_scope(""), None);

    let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
    let c = cache(&mock, Duration::ZERO);
    let t = token(KEY1_PEM, "k1", claims("staff", 300, ISSUER));
    assert_eq!(
        verify_platform_token(&c, &t, Some(ISSUER))
            .await
            .unwrap()
            .role,
        Role::Manager
    );
    let none = token(KEY1_PEM, "k1", claims("read", 300, ISSUER));
    assert!(
        verify_platform_token(&c, &none, Some(ISSUER))
            .await
            .is_err()
    );
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
        assert!(
            verify_platform_token(&c, &bogus, Some(ISSUER))
                .await
                .is_err()
        );
    }
    assert_eq!(
        mock.hits.load(Ordering::SeqCst),
        1,
        "one fetch, not one per token"
    );
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
    assert!(
        verify_platform_token(&c, &forged, Some(ISSUER))
            .await
            .is_err()
    );

    // Payload swapped after signing (owner -> different tenant).
    let good = token(KEY1_PEM, "k1", claims("staff", 300, ISSUER));
    let other = token(
        KEY1_PEM,
        "k1",
        json!({
            "sub": "acct_1", "tid": "tnt_OTHER", "scope": "owner", "iss": ISSUER,
            "exp": chrono::Utc::now().timestamp() + 300,
        }),
    );
    let mut parts: Vec<&str> = good.split('.').collect();
    let other_parts: Vec<&str> = other.split('.').collect();
    parts[1] = other_parts[1];
    let tampered = parts.join(".");
    assert!(
        verify_platform_token(&c, &tampered, Some(ISSUER))
            .await
            .is_err()
    );
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
    assert!(
        verify_platform_token(&c, &no_kid, Some(ISSUER))
            .await
            .is_err()
    );
}

fn test_config(
    jwks_url: Option<String>,
    tenant_mode: crate::core::config::TenantMode,
) -> crate::core::config::Config {
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
        identity_tenant_id: None,
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
    use crate::core::config::TenantMode;
    use crate::core::middleware::auth::verify_bearer;
    use crate::core::tenancy::Tenant;
    let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
    let config = test_config(Some(mock.url.clone()), TenantMode::Multi);
    let exp = chrono::Utc::now().timestamp() + 300;

    // Local HS256 token carrying a tenant.
    let local = hs256(
        &config.jwt_secret,
        json!({ "sub": "u1", "exp": exp, "tid": "tnt_local" }),
    );
    let v = verify_bearer(&bearer(&local), &config).await.unwrap();
    assert_eq!(v.tenant, Some(Tenant::id("tnt_local").unwrap()));
    assert_eq!(v.tenant_id().as_deref(), Some("tnt_local"));

    // Local token without a tenant (or an empty one): no tenant, which is
    // what makes CurrentUser reject it in multi mode.
    for claims in [
        json!({ "sub": "u1", "exp": exp }),
        json!({ "sub": "u1", "exp": exp, "tid": "  " }),
    ] {
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
    let bad = hs256(
        "some-other-secret-some-other-secret-0000",
        json!({ "sub": "u1", "exp": exp, "tid": "t" }),
    );
    assert!(verify_bearer(&bearer(&bad), &config).await.is_err());
}

#[tokio::test]
async fn platform_tokens_are_rejected_when_not_enabled() {
    use crate::core::config::TenantMode;
    use crate::core::middleware::auth::verify_bearer;
    let config = test_config(None, TenantMode::Multi);
    let platform = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));
    assert!(verify_bearer(&bearer(&platform), &config).await.is_err());
    assert!(
        verify_bearer(&axum::http::HeaderMap::new(), &config)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn single_shop_mode_only_accepts_platform_tokens_for_its_own_shop() {
    use crate::core::config::TenantMode;
    use crate::core::middleware::auth::verify_bearer;
    let mock = mock_jwks(jwks(&[("k1", KEY1_X)])).await;
    // `claims` mints tokens for tenant `tnt_1`.
    let platform = token(KEY1_PEM, "k1", claims("owner", 300, ISSUER));

    // No shop configured: platform tokens are refused outright.
    let config = test_config(Some(mock.url.clone()), TenantMode::Single);
    assert!(verify_bearer(&bearer(&platform), &config).await.is_err());

    // A different shop: refused.
    let mut config = test_config(Some(mock.url.clone()), TenantMode::Single);
    config.identity_tenant_id = Some("tnt_OTHER".to_string());
    assert!(verify_bearer(&bearer(&platform), &config).await.is_err());

    // This shop: accepted.
    config.identity_tenant_id = Some("tnt_1".to_string());
    let v = verify_bearer(&bearer(&platform), &config).await.unwrap();
    assert_eq!(v.role, Some(Role::Admin));
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
