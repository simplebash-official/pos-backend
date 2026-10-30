use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::billing::model::CreditNoteDocument;

pub(super) fn credit_notes_collection(db: &TenantDatabase) -> ScopedCollection<CreditNoteDocument> {
    db.collection("credit_notes")
}
