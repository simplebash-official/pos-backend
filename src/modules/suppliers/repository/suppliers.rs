use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::suppliers::model::SupplierDocument;

pub(super) fn suppliers(db: &TenantDatabase) -> ScopedCollection<SupplierDocument> {
    db.collection("suppliers")
}
