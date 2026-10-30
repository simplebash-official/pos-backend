use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::purchases::model::PurchaseReceiptDocument;

pub(super) fn receipts(db: &TenantDatabase) -> ScopedCollection<PurchaseReceiptDocument> {
    db.collection("purchase_receipts")
}
