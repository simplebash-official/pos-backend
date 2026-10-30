use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::billing::model::InvoiceDocument;

pub(super) fn invoices(db: &TenantDatabase) -> ScopedCollection<InvoiceDocument> {
    db.collection("invoices")
}
