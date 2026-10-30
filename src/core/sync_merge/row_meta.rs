#![forbid(unsafe_code)]

/// What the receiver knows about a row (from `sync_row_meta` / Mongo fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMeta {
    pub updated_at_ms: i64,
    pub device_id: String,
    pub version: i64,
    pub deleted: bool,
}
