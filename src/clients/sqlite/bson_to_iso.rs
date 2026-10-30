/// Formats a `mongodb::bson::DateTime` into an RFC-3339 string.
#[must_use]
pub fn bson_to_iso(dt: &mongodb::bson::DateTime) -> String {
    dt.to_chrono().to_rfc3339()
}
