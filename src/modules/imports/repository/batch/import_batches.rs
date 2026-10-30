use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::imports::model::ImportBatchDocument;

pub(super) fn import_batches(db: &TenantDatabase) -> ScopedCollection<ImportBatchDocument> {
    db.collection("import_batches")
}
