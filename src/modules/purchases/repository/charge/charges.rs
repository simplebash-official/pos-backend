use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::purchases::model::ReceiptChargeDocument;

pub(super) fn charges(db: &TenantDatabase) -> ScopedCollection<ReceiptChargeDocument> {
    db.collection("receipt_charges")
}
