use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;

pub(super) fn templates(db: &TenantDatabase) -> ScopedCollection<RecurringExpenseDocument> {
    db.collection("recurring_expenses")
}
