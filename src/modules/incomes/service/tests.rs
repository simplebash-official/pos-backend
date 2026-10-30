use super::*;

#[test]
fn days_must_be_real_dates() {
    assert!(parse_day("date", "2026-10-23").is_ok());
    assert!(parse_day("date", "2026-02-30").is_err());
}

#[test]
fn text_is_trimmed_and_limited() {
    assert_eq!(
        clean("Note", Some("  hi "), 10).unwrap().as_deref(),
        Some("hi")
    );
    assert_eq!(clean("Note", Some("   "), 10).unwrap(), None);
    assert!(clean("Note", Some("abcdefghijk"), 10).is_err());
}
