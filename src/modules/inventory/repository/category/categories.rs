use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::CategoryDocument;

pub(super) fn categories(db: &TenantDatabase) -> ScopedCollection<CategoryDocument> {
    db.collection("categories")
}
