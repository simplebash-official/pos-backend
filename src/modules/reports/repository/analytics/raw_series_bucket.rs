use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// Time series
// ---------------------------------------------------------------------------

/// One raw time bucket straight from Mongo, keyed by its start instant.
#[derive(Debug, Clone, Default)]
pub(crate) struct RawSeriesBucket {
    pub period_start: DateTime<Utc>,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub cogs_cents: i64,
    pub discount_cents: i64,
    pub invoice_count: u64,
}
