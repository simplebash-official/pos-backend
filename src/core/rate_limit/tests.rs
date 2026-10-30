use super::failure_limiter::FailureLimiter;
use super::limit_requests::limit_requests;
use super::request_limits::RequestLimits;
use super::token_buckets::TokenBuckets;

use std::time::{Duration, Instant};

#[test]
fn blocks_after_max_failures_and_reset_clears_it() {
    let limiter = FailureLimiter::new(3, Duration::from_secs(60));
    for _ in 0..2 {
        limiter.record_failure("k");
        assert!(limiter.check("k").is_ok());
    }
    limiter.record_failure("k");
    assert!(limiter.check("k").is_err());
    assert!(limiter.check("other").is_ok(), "keys are independent");

    limiter.reset("k");
    assert!(limiter.check("k").is_ok());
}

#[test]
fn an_expired_window_unblocks() {
    let limiter = FailureLimiter::new(1, Duration::from_millis(20));
    limiter.record_failure("k");
    assert!(limiter.check("k").is_err());
    std::thread::sleep(Duration::from_millis(30));
    assert!(limiter.check("k").is_ok());
    // A failure after expiry starts a new window at 1, which (max 1)
    // blocks again — the count did not carry over from the old window.
    limiter.record_failure("k");
    assert!(limiter.check("k").is_err());
}

#[test]
fn live_calculations_have_their_own_bucket() {
    let limits = RequestLimits::new((1.0, 0.0), (1.0, 0.0), (2.0, 0.0));
    assert!(limits.allow("a", "/api/calc/receipt"));
    assert!(limits.allow("a", "/api/calc/receipt"));
    assert!(!limits.allow("a", "/api/calc/receipt"), "live burst spent");
    assert!(
        limits.allow("a", "/api/billing"),
        "general bucket untouched"
    );
}

#[test]
fn sensitive_paths_are_limited_harder_and_per_client() {
    let limits = RequestLimits::new((100.0, 0.0), (2.0, 0.0), (3.0, 0.0));
    assert!(limits.allow("a", "/api/auth/login"));
    assert!(limits.allow("a", "/api/auth/login"));
    assert!(
        !limits.allow("a", "/api/auth/login"),
        "sensitive burst spent"
    );
    assert!(
        limits.allow("a", "/api/billing"),
        "general routes unaffected"
    );
    assert!(
        limits.allow("b", "/api/auth/login"),
        "other clients unaffected"
    );
}

#[test]
fn general_bucket_runs_dry_and_refills() {
    let b = TokenBuckets::new(2.0, 1.0);
    let t = Instant::now();
    assert!(b.take("k", t) && b.take("k", t));
    assert!(!b.take("k", t));
    assert!(b.take("k", t + Duration::from_secs(1)));
}

#[tokio::test]
async fn middleware_answers_429_with_the_error_envelope() {
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    let limits = std::sync::Arc::new(RequestLimits::new((100.0, 0.0), (1.0, 0.0), (3.0, 0.0)));
    let app = Router::new()
        .route("/api/auth/shop/x", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(limits, limit_requests));
    let call = |ip: &'static str| {
        let app = app.clone();
        async move {
            app.oneshot(
                Request::builder()
                    .uri("/api/auth/shop/x")
                    .header("x-real-ip", ip)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        }
    };
    assert_eq!(call("1.1.1.1").await.status(), 200);
    let blocked = call("1.1.1.1").await;
    assert_eq!(blocked.status(), 429);
    let body = axum::body::to_bytes(blocked.into_body(), 4096)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("RATE_LIMITED"));
    assert_eq!(call("2.2.2.2").await.status(), 200, "per-client buckets");
}
