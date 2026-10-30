use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::SubcategoryDocument;

pub(super) fn subcategories(db: &TenantDatabase) -> ScopedCollection<SubcategoryDocument> {
    db.collection("subcategories")
}
