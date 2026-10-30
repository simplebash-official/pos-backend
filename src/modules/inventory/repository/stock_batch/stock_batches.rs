use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::inventory::model::StockBatchDocument;

pub(super) fn stock_batches(db: &TenantDatabase) -> ScopedCollection<StockBatchDocument> {
    db.collection("stock_batches")
}
