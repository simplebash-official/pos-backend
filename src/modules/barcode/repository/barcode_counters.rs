use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::barcode::model::BarcodeCounterDocument;

pub(super) fn barcode_counters(db: &TenantDatabase) -> ScopedCollection<BarcodeCounterDocument> {
    db.collection("barcode_counters")
}
