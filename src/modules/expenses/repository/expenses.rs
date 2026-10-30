use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::expenses::model::ExpenseDocument;

pub(super) fn expenses(db: &TenantDatabase) -> ScopedCollection<ExpenseDocument> {
    db.collection("expenses")
}
