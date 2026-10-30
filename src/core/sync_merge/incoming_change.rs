#![forbid(unsafe_code)]

use crate::domain::sync_v2::ChangeRecord;

/// A change with its (possibly clamped) timestamp in epoch milliseconds.
#[derive(Debug, Clone)]
pub struct IncomingChange<'a> {
    pub record: &'a ChangeRecord,
    pub updated_at_ms: i64,
    pub clamped: bool,
}
