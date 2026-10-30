use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::expenses::model::ExpensePaymentDocument;

pub(super) fn payments(db: &TenantDatabase) -> ScopedCollection<ExpensePaymentDocument> {
    db.collection("expense_payments")
}
