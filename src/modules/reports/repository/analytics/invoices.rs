use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use mongodb::bson::Document;

pub(super) fn invoices(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("invoices")
}
