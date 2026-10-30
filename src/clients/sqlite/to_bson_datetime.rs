use super::parse_iso_datetime::parse_iso_datetime;

/// Converts an RFC-3339 string directly into `mongodb::bson::DateTime`.
#[must_use]
pub fn to_bson_datetime(s: &str) -> mongodb::bson::DateTime {
    mongodb::bson::DateTime::from_chrono(parse_iso_datetime(s))
}
