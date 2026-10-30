use chrono::{DateTime, Utc};

/// A product's latest label: when, and how many stickers it got in that print.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LastLabel {
    pub(super) at: DateTime<Utc>,
    pub(super) copies: u32,
}
