use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::print_jobs::model::PrintJobDocument;

pub(super) fn print_jobs(db: &TenantDatabase) -> ScopedCollection<PrintJobDocument> {
    db.collection("print_jobs")
}
