use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::billing::model::PaymentDocument;

pub(super) fn payments(db: &TenantDatabase) -> ScopedCollection<PaymentDocument> {
    db.collection("payments")
}
