use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::StockMovementDocument;

pub(super) fn stock_movements(db: &TenantDatabase) -> ScopedCollection<StockMovementDocument> {
    db.collection("stock_movements")
}
