use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::documents::model::GeneratedDocumentDocument;

pub(super) fn generated_documents(
    db: &TenantDatabase,
) -> ScopedCollection<GeneratedDocumentDocument> {
    db.collection("generated_documents")
}
