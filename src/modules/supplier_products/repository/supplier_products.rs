use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::supplier_products::model::SupplierProductLinkDocument;

pub(super) fn supplier_products(
    db: &TenantDatabase,
) -> ScopedCollection<SupplierProductLinkDocument> {
    db.collection("supplier_products")
}
