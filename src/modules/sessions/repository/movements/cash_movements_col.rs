use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::sessions::model::CashMovementDocument;

pub(super) fn cash_movements_col(db: &TenantDatabase) -> ScopedCollection<CashMovementDocument> {
    db.collection("cash_movements")
}
