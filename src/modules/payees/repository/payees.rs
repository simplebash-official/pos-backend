use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::payees::model::PayeeDocument;

pub(super) fn payees(db: &TenantDatabase) -> ScopedCollection<PayeeDocument> {
    db.collection("payees")
}
