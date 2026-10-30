/// Parses an RFC-3339 timestamp string into chrono `DateTime<Utc>`.
#[must_use]
pub fn parse_iso_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}
