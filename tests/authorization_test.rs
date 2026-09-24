// Whole-surface authorization audit. Unlike the per-module test files, this
// one asserts nothing about business behavior — it walks *every* route in the
// generated OpenAPI document and checks that each one either requires a token
// or is on an explicit allowlist of intentionally-public routes.
//
// This exists because authentication in this codebase is opt-in per handler:
// a route is protected only because its handler declares `CurrentUser` or
// `AdminUser` as an argument (see `core::middleware::auth`). Forgetting that
// argument produces a silently public endpoint that still compiles, still
// passes every other test, and is still documented accurately by the utoipa
// derive — which is how the whole `/api/inventory` product and stock surface
// (reads, creates, price edits, stock adjustments, bulk delete) was reachable
// with no token at all. A per-module test can't catch that class of bug,
// because the module that forgets the extractor is also the module that
// forgets to test for it.
//
// Deriving the route list from `/api-docs/openapi.json` rather than hardcoding
// it is what makes this hold going forward: a new route registered through
// `routes!()` is audited the moment it exists, with no edit here. The only
// maintenance is adding a genuinely-public route to `PUBLIC_ROUTES` below,
// which is a deliberate act rather than an omission.
//
// Needs no live MongoDB: an unauthenticated request is rejected by the
// extractor before any handler touches the database (same construction as
// `openapi_test.rs` — the client is built but never pinged).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use simplebash_pos_backend::{app, app::AppState, clients, core::config::Config};
use mongodb::Client;
use mongodb::options::{ClientOptions, ResolverConfig};
use tower::ServiceExt;

/// Every route that is allowed to answer without a token, as
/// `(method, path)` exactly as the OpenAPI document spells them.
///
/// Keep this in sync with the same list in `app::build_router`'s comment.
/// Adding an entry here is the one way to make a public route pass this
/// audit, so it doubles as the reviewable record of that decision:
///   - the health probe and the module-status stubs return static payloads
///     and touch no data;
///   - login is the endpoint that issues the token, so it cannot require one.
///
/// Swagger's own paths (`/docs`, `/api-docs/openapi.json`) aren't listed
/// because they don't appear in the document this test walks.
///
/// `/api/repairs` and `/api/print-jobs` were removed from this list once
/// those modules gained real handlers (Phase 3 of the billing-backend
/// migration) — their `GET /` is now the real "list" endpoint, gated by
/// `CurrentUser`, not a public status stub.
const PUBLIC_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/health"),
    ("POST", "/api/auth/login"),
    ("GET", "/api/billing"),
    ("GET", "/api/inventory"),
    ("GET", "/api/reports"),
    ("GET", "/api/system/setup-status"),
    ("POST", "/api/system/setup"),
    // Server-to-server: authenticated by the shared X-Provision-Secret header, not a
    // bearer token, and blocked from the internet at nginx.
    ("POST", "/api/internal/provision"),
];

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration for test run");
    let db_handle = match config.database_type {
        simplebash_pos_backend::core::config::DatabaseType::Sqlite => {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .expect("in-memory sqlite pool");
            simplebash_pos_backend::clients::db::Db::Sqlite(pool)
        }
        simplebash_pos_backend::core::config::DatabaseType::Mongo => {
            let uri = if config.mongodb_uri.is_empty() {
                "mongodb://localhost:27017"
            } else {
                &config.mongodb_uri
            };
            let parse_res = ClientOptions::parse(uri).await;
            let options = match parse_res {
                Ok(opts) => opts,
                Err(_) => match ClientOptions::parse(uri)
                    .resolver_config(ResolverConfig::cloudflare())
                    .await
                {
                    Ok(opts) => opts,
                    Err(_) => ClientOptions::parse(uri)
                        .resolver_config(ResolverConfig::google())
                        .await
                        .expect("valid mongodb uri"),
                },
            };
            let client = Client::with_options(options).expect("client construction");
            let db_name = if config.mongodb_db_name.is_empty() {
                "simplebash_pos_test"
            } else {
                &config.mongodb_db_name
            };
            let db = client.database(db_name);
            simplebash_pos_backend::clients::db::Db::from_mongo(db)
        }
    };

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config,
        db: db_handle,
        document_server,
        reports_engine,
    };
    app::build_router(state)
}

/// One `(method, path, declares_security)` row per operation in the OpenAPI
/// document — the full route surface, straight from what `routes!()`
/// registered.
async fn all_routes(router: &axum::Router) -> Vec<(String, String, bool)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let mut routes = Vec::new();
    for (path, item) in json["paths"].as_object().expect("paths object") {
        for (method, operation) in item.as_object().expect("path item object") {
            // Path items also carry non-operation keys (`parameters`,
            // `summary`); only real HTTP verbs are routes.
            if !matches!(
                method.as_str(),
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
            ) {
                continue;
            }
            let declares_security = operation
                .get("security")
                .and_then(|s| s.as_array())
                .is_some_and(|s| !s.is_empty());
            routes.push((method.to_uppercase(), path.clone(), declares_security));
        }
    }

    assert!(
        routes.len() > 40,
        "expected the full route surface, got {} — did the spec fail to \
         build?",
        routes.len()
    );
    routes
}

/// Fills `{id}`-style path params with a syntactically plausible value. The
/// value never matters: on a gated route the auth extractor rejects the
/// request before the handler parses the path, and on a public route no
/// parameterized path exists.
fn concrete_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut in_param = false;
    for ch in path.chars() {
        match ch {
            '{' => in_param = true,
            '}' => {
                in_param = false;
                out.push_str("placeholder");
            }
            _ if !in_param => out.push(ch),
            _ => {}
        }
    }
    out
}

async fn status_without_token(router: &axum::Router, method: &str, path: &str) -> StatusCode {
    let request = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(concrete_path(path))
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::empty())
        .unwrap();

    router.clone().oneshot(request).await.unwrap().status()
}

fn is_public(method: &str, path: &str) -> bool {
    PUBLIC_ROUTES
        .iter()
        .any(|(m, p)| *m == method && *p == path)
}

#[tokio::test]
async fn every_route_requires_a_token_unless_explicitly_public() {
    let router = build_test_app().await;
    let mut unprotected = Vec::new();

    for (method, path, _) in all_routes(&router).await {
        if is_public(&method, &path) {
            continue;
        }

        let status = status_without_token(&router, &method, &path).await;

        // 401 is the only acceptable answer to an anonymous request here.
        // Anything else means either the route is public, or an auth
        // extractor is declared *after* a body/path extractor that rejected
        // first — auth must be evaluated before the request is parsed, so
        // both are failures worth surfacing.
        if status != StatusCode::UNAUTHORIZED {
            unprotected.push(format!("{method} {path} -> {status}"));
        }
    }

    assert!(
        unprotected.is_empty(),
        "these routes answered an unauthenticated request instead of 401.\n\
         If a route is meant to be public, add it to PUBLIC_ROUTES here and to \
         the list in `app::build_router`. Otherwise give its handler a \
         `CurrentUser`/`AdminUser` argument, declared before any `Json`/`Path` \
         extractor:\n  {}",
        unprotected.join("\n  ")
    );
}

#[tokio::test]
async fn intentionally_public_routes_stay_reachable_without_a_token() {
    let router = build_test_app().await;

    // The other half of the guard: without this, "protect everything"
    // trivially passes the audit above while breaking login and the health
    // probe. Asserting only "not 401" keeps it robust — `POST /auth/login`
    // with an empty body legitimately answers 4xx for a *different* reason.
    for (method, path) in PUBLIC_ROUTES {
        let status = status_without_token(&router, method, path).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} is documented as public but demanded a token"
        );
    }
}

#[tokio::test]
async fn openapi_security_annotations_match_actual_enforcement() {
    let router = build_test_app().await;
    let mut mismatched = Vec::new();

    // The Swagger page is the contract clients build against. A route that is
    // enforced but undocumented sends integrators down a dead end; one that is
    // documented but unenforced advertises a lock that isn't there. Both are
    // silent today because the `security(...)` annotation and the handler
    // argument are written independently.
    for (method, path, declares_security) in all_routes(&router).await {
        let enforced =
            status_without_token(&router, &method, &path).await == StatusCode::UNAUTHORIZED;

        if enforced != declares_security {
            let (doc, real) = if declares_security {
                ("documents `security(bearerAuth)`", "does not enforce it")
            } else {
                ("omits `security(bearerAuth)`", "does enforce auth")
            };
            mismatched.push(format!("{method} {path} {doc} but {real}"));
        }
    }

    assert!(
        mismatched.is_empty(),
        "OpenAPI security annotations disagree with real behavior:\n  {}",
        mismatched.join("\n  ")
    );
}
