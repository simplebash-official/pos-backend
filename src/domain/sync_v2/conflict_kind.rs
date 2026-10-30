use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Kinds of automatically resolved but reviewable conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    LwwLoser,
    SerialDoubleSold,
    OverRefund,
    NegativeStock,
    UniqueViolation,
}

impl ConflictKind {
    /// Stable string stored in `sync_conflicts.kind`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictKind::LwwLoser => "LWW_LOSER",
            ConflictKind::SerialDoubleSold => "SERIAL_DOUBLE_SOLD",
            ConflictKind::OverRefund => "OVER_REFUND",
            ConflictKind::NegativeStock => "NEGATIVE_STOCK",
            ConflictKind::UniqueViolation => "UNIQUE_VIOLATION",
        }
    }
}
