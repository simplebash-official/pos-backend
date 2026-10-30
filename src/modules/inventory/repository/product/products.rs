use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::ProductDocument;

pub(super) fn products(db: &TenantDatabase) -> ScopedCollection<ProductDocument> {
    db.collection("products")
}
