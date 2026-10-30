use crate::core::sync_merge::RowMeta;

use serde_json::Value;

/// One stored row: merge metadata plus the DTO payload.
#[derive(Debug, Clone)]
pub struct SimRow {
    pub meta: RowMeta,
    pub payload: Value,
}
