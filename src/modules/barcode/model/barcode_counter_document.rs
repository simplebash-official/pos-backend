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
