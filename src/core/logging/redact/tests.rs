use super::logging_redact::REDACTED;
use super::logging_redact::body_for_log;
use super::logging_redact::redact_json_text;
use super::logging_redact::redact_value;

use serde_json::json;

#[test]
fn masks_credentials_keeps_customer_data() {
    let mut v = json!({
        "email": "owner@shop.lk",
        "password": "hunter2",
        "adminPassword": "x",
        "customer": {"phone": "0771234567", "address": "Kandy"},
        "data": {"token": "abc", "items": [{"card_number": "4111"}]}
    });
    redact_value(&mut v);
    assert_eq!(v["email"], "owner@shop.lk");
    assert_eq!(v["password"], REDACTED);
    assert_eq!(v["adminPassword"], REDACTED);
    assert_eq!(v["customer"]["phone"], "0771234567");
    assert_eq!(v["data"]["token"], REDACTED);
    assert_eq!(v["data"]["items"][0]["card_number"], REDACTED);
}

#[test]
fn huge_bodies_are_scrubbed_as_text_without_parsing() {
    let mut body = String::from(r#"{"email":"a@b.c","adminPassword":"hunter2","items":["#);
    for i in 0..20_000 {
        body.push_str(&format!(
            r#"{{"key":"prod_{i}","api_key":"k{i}","name":"Item"}},"#
        ));
    }
    body.push_str("{}]}");
    let out = body_for_log(body.as_bytes(), 1024);
    // cap + growth from "[REDACTED]" replacements + the size suffix
    assert!(out.len() < 1024 + 512, "capped: {}", out.len());
    assert!(out.contains("a@b.c"));
    assert!(!out.contains("hunter2"));
    assert!(!out.contains("\"k0\""));
    assert!(out.contains("bytes total]"));
}

#[test]
fn json_text_redaction_checks_whole_keys() {
    let out = redact_json_text(r#"{"shipping":"fast","pin":1234,"token":"abc","n":5}"#);
    assert!(out.contains(r#""shipping":"fast""#));
    assert!(out.contains(r#""pin":"[REDACTED]""#));
    assert!(out.contains(r#""token":"[REDACTED]""#));
    assert!(out.contains(r#""n":5"#));
}

#[test]
fn body_for_log_redacts_and_caps() {
    let body = br#"{"email":"a@b.c","password":"p"}"#;
    let out = body_for_log(body, 1024);
    assert!(out.contains("a@b.c") && out.contains(REDACTED) && !out.contains("\"p\""));

    // Within 4x the cap: parsed, redacted, then cut.
    let medium = format!("{{\"note\":\"{}\"}}", "x".repeat(60));
    assert!(body_for_log(medium.as_bytes(), 20).ends_with("…[truncated]"));
    // Beyond 4x the cap: scrubbed as text, never parsed.
    let long = format!("{{\"note\":\"{}\"}}", "x".repeat(100));
    assert!(body_for_log(long.as_bytes(), 20).ends_with("bytes total]"));

    let jwt = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1In0.sig";
    assert!(!body_for_log(jwt.as_bytes(), 1024).contains("eyJhbGci"));
}
