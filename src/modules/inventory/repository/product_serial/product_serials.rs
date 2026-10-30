use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::ProductSerialDocument;

pub(super) fn product_serials(db: &TenantDatabase) -> ScopedCollection<ProductSerialDocument> {
    db.collection("product_serials")
}
