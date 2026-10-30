use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::balances::model::BalanceOpeningDocument;

pub(super) fn openings(db: &TenantDatabase) -> ScopedCollection<BalanceOpeningDocument> {
    db.collection("balance_openings")
}
