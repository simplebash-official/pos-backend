/// Formats the current UTC timestamp as an ISO-8601/RFC-3339 string.
#[must_use]
pub fn now_utc_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
