use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::purchases::model::SupplierCreditDocument;

pub(super) fn credits(db: &TenantDatabase) -> ScopedCollection<SupplierCreditDocument> {
    db.collection("supplier_credits")
}
