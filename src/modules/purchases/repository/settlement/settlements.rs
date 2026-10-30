use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::purchases::model::ReceiptSettlementDocument;

pub(super) fn settlements(db: &TenantDatabase) -> ScopedCollection<ReceiptSettlementDocument> {
    db.collection("receipt_settlements")
}
