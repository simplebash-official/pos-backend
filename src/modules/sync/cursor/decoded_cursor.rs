use chrono::{DateTime, Utc};

/// Decoded representation of a sync pagination cursor.
#[derive(Debug, Clone)]
pub struct DecodedCursor {
    /// Timestamp boundary parsed from the cursor.
    pub timestamp: DateTime<Utc>,
    /// Optional tie-breaker document key parsed from the cursor.
    pub key: Option<String>,
}
