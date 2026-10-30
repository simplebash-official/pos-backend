use super::parse_date_range_tz::parse_date_range_tz;

use chrono::Duration;

#[test]
fn today_bounds_are_local_midnight_in_utc() {
    let (start, end) = parse_date_range_tz(Some("today"), None, None).unwrap();
    // 24h window, and start is 18:30 UTC of the previous day (00:00 +05:30).
    assert_eq!(end - start, Duration::days(1));
    assert_eq!(start.format("%H:%M").to_string(), "18:30");
}

#[test]
fn custom_ymd_is_interpreted_in_shop_local_time() {
    let (start, end) =
        parse_date_range_tz(Some("custom"), Some("2026-08-01"), Some("2026-08-31")).unwrap();
    assert_eq!(start.to_rfc3339(), "2026-07-31T18:30:00+00:00");
    // end bound is exclusive: local midnight of 1 Sep.
    assert_eq!(end.to_rfc3339(), "2026-08-31T18:30:00+00:00");
}
