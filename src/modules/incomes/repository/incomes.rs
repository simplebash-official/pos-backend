use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::incomes::model::IncomeDocument;

pub(super) fn incomes(db: &TenantDatabase) -> ScopedCollection<IncomeDocument> {
    db.collection("incomes")
}
