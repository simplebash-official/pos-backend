#![forbid(unsafe_code)]

use serde_json::Value;

/// Outcome of merging the lifecycle columns of an invoice / credit note.
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleMerge {
    /// The local payload with the winning lifecycle columns applied.
    pub merged: Value,
    /// Whether `merged` differs from the local payload.
    pub changed: bool,
    /// `Some("IMMUTABLE_MISMATCH" | "INVALID_PAYLOAD")` when the change is refused.
    pub rejected: Option<&'static str>,
}
