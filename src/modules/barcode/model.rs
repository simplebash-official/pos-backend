// Mongo document shape for the `barcode` module's counter table. This is a
// pure internal sequence table, not a business entity — like
// `inventory::repository::sku_counter::SkuCounterDocument`, it deliberately
// has no `key`/`created_at`/`updated_at` fields (see CLAUDE.md's documented
// exception for counter/sequence tables): its `_id` (`namespace`) already
// is the meaningful key.

use serde::{Deserialize, Serialize};

/// Mongo document for barcode auto-increment sequence counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BarcodeCounterDocument {
    /// Namespace identifier for barcode sequence (e.g. prefix).
    #[serde(rename = "_id")]
    pub(crate) namespace: String,
    /// Last sequentially generated numeric value for this namespace.
    pub(crate) seq: i64,
}
