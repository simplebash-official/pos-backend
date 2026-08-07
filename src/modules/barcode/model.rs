// Mongo document shape for the `barcode` module's counter table. This is a
// pure internal sequence table, not a business entity — like
// `inventory::repository::sku_counter::SkuCounterDocument`, it deliberately
// has no `key`/`created_at`/`updated_at` fields (see CLAUDE.md's documented
// exception for counter/sequence tables): its `_id` (`namespace`) already
// is the meaningful key.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BarcodeCounterDocument {
    #[serde(rename = "_id")]
    pub(crate) namespace: String,
    pub(crate) seq: i64,
}
