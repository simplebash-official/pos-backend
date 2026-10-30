use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::balances::model::AdvanceRecoveryDocument;

pub(super) fn recoveries(db: &TenantDatabase) -> ScopedCollection<AdvanceRecoveryDocument> {
    db.collection("advance_recoveries")
}
