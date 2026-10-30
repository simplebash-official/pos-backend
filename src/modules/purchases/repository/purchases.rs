use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::purchases::model::PurchaseDocument;

pub(super) fn purchases(db: &TenantDatabase) -> ScopedCollection<PurchaseDocument> {
    db.collection("purchases")
}
