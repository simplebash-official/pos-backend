use super::build_cache_key::build_cache_key;
use super::build_scoped_cache_key::build_scoped_cache_key;
use super::reports_cache::ReportsCache;
use super::tenant_tag::tenant_tag;
use chrono::Duration;

use serde_json::json;

#[test]
fn single_shop_key_is_unchanged() {
    assert_eq!(
        build_scoped_cache_key(None, "feed", "a:b"),
        build_cache_key("feed", "a:b")
    );
}

#[test]
fn tenants_get_distinct_keys_for_identical_reports() {
    let a = build_scoped_cache_key(Some("shop_a"), "feed", "overview:today");
    let b = build_scoped_cache_key(Some("shop_b"), "feed", "overview:today");
    assert_ne!(a, b);
    assert_ne!(a, build_scoped_cache_key(None, "feed", "overview:today"));
}

#[test]
fn invalidate_matching_only_evicts_the_callers_tenant() {
    let cache = ReportsCache::new();
    let ttl = Duration::seconds(60);
    for t in ["shop_a", "shop_b"] {
        cache.set(
            build_scoped_cache_key(Some(t), "feed", "x"),
            json!(t),
            ttl,
            vec!["active".into(), tenant_tag(t)],
        );
    }
    assert_eq!(
        cache.invalidate_matching(&["active", &tenant_tag("shop_a")]),
        1
    );
    assert!(
        cache
            .get(&build_scoped_cache_key(Some("shop_a"), "feed", "x"))
            .is_none()
    );
    assert!(
        cache
            .get(&build_scoped_cache_key(Some("shop_b"), "feed", "x"))
            .is_some()
    );
}
