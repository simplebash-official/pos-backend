use super::notification_query::MAX_SEARCH_CHARS;
use super::*;
use chrono::{TimeZone, Utc};

fn alert() -> NewNotification {
    NewNotification {
        category: NotificationCategory::Repairs,
        severity: NotificationSeverity::Success,
        title: "  Ready for pickup ".to_string(),
        body: " iPhone 12 is ready ".to_string(),
        link: Some(NotificationLink {
            path: "/repairs".to_string(),
            label: " Open job ".to_string(),
        }),
        audience: NotificationAudience::Everyone,
        source: NotificationSource::Shop,
        dedupe_key: Some(" repair:rep_1:ready ".to_string()),
        raised_by_user_key: None,
    }
}

#[test]
fn validation_trims_every_text_field() {
    let checked = alert()
        .validated()
        .expect("a well-formed alert is accepted");
    assert_eq!(checked.title, "Ready for pickup");
    assert_eq!(checked.body, "iPhone 12 is ready");
    assert_eq!(checked.link.expect("link kept").label, "Open job");
    assert_eq!(checked.dedupe_key.as_deref(), Some("repair:rep_1:ready"));
}

#[test]
fn empty_and_too_long_text_is_refused() {
    let mut empty = alert();
    empty.title = "   ".to_string();
    assert!(empty.validated().is_err());

    let mut long = alert();
    long.body = "x".repeat(MAX_BODY_CHARS + 1);
    assert!(long.validated().is_err());

    let mut exact = alert();
    exact.title = "é".repeat(MAX_TITLE_CHARS);
    assert!(
        exact.validated().is_ok(),
        "the limit counts characters, not bytes"
    );
}

#[test]
fn a_link_must_stay_inside_the_app() {
    for path in [
        "https://evil.example",
        "//evil.example",
        "repairs",
        "javascript:alert(1)",
    ] {
        let mut bad = alert();
        bad.link = Some(NotificationLink {
            path: path.to_string(),
            label: "Open".to_string(),
        });
        assert!(bad.validated().is_err(), "{path} must be refused");
    }
}

#[test]
fn derived_keys_are_stable_and_distinct() {
    let a = notification_key_for("low-stock:prod_1:sm_1");
    assert_eq!(a, notification_key_for("low-stock:prod_1:sm_1"));
    assert_ne!(a, notification_key_for("low-stock:prod_1:sm_2"));
    assert!(a.starts_with("ntf_"));

    let read = read_key_for("usr_1", &a);
    assert_eq!(read, read_key_for("usr_1", &a));
    assert_ne!(read, read_key_for("usr_2", &a));
    assert!(read.starts_with("nrd_"));
}

#[test]
fn names_round_trip() {
    for category in NotificationCategory::ALL {
        assert_eq!(
            NotificationCategory::parse(category.as_str()),
            Some(category)
        );
    }
    for severity in NotificationSeverity::ALL {
        assert_eq!(
            NotificationSeverity::parse(severity.as_str()),
            Some(severity)
        );
    }
    assert_eq!(NotificationCategory::parse("billing"), None);
}

#[test]
fn the_filter_parses_lists_and_refuses_bad_input() {
    let filter = NotificationFilter::parse(
        Some("sales, repairs,"),
        Some("critical"),
        None,
        None,
        Some("  phone "),
    )
    .expect("valid filter");
    assert_eq!(
        filter.categories,
        vec![NotificationCategory::Sales, NotificationCategory::Repairs]
    );
    assert_eq!(filter.severities, vec![NotificationSeverity::Critical]);
    assert_eq!(filter.search.as_deref(), Some("phone"));

    assert!(NotificationFilter::parse(Some("billing"), None, None, None, None).is_err());
    let day = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).single();
    assert!(NotificationFilter::parse(None, None, day, day, None).is_err());
    let long = "x".repeat(MAX_SEARCH_CHARS + 1);
    assert!(NotificationFilter::parse(None, None, None, None, Some(&long)).is_err());
    let blank = NotificationFilter::parse(None, None, None, None, Some("  ")).expect("ok");
    assert_eq!(blank.search, None);
}

#[test]
fn the_cursor_round_trips_and_refuses_garbage() {
    let cursor = HistoryCursor {
        created_at: Utc
            .timestamp_millis_opt(1_763_000_000_123)
            .single()
            .expect("valid time"),
        key: "ntf_abc".to_string(),
    };
    assert_eq!(HistoryCursor::decode(&cursor.encode()), Ok(cursor));
    for bad in ["", "123", "abc:ntf_1", "123:"] {
        assert!(HistoryCursor::decode(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_audience_wire_shape_is_tagged() {
    let json = serde_json::to_value(NotificationAudience::User {
        user_key: "usr_1".to_string(),
    })
    .expect("serializes");
    assert_eq!(
        json,
        serde_json::json!({"type": "user", "userKey": "usr_1"})
    );
    let parsed: NotificationAudience =
        serde_json::from_value(serde_json::json!({"type": "permission", "permission": "a:b"}))
            .expect("parses");
    assert_eq!(
        parsed,
        NotificationAudience::Permission {
            permission: "a:b".to_string()
        }
    );
}
