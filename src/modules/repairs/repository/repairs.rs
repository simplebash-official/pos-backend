use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::repairs::model::RepairDocument;

pub(super) fn repairs(db: &TenantDatabase) -> ScopedCollection<RepairDocument> {
    db.collection("repairs")
}
