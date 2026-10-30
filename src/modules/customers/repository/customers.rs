use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::customers::model::CustomerDocument;

pub(super) fn customers(db: &TenantDatabase) -> ScopedCollection<CustomerDocument> {
    db.collection("customers")
}
