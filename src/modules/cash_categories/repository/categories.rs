use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::cash_categories::model::CashCategoryDocument;

pub(super) fn categories(db: &TenantDatabase) -> ScopedCollection<CashCategoryDocument> {
    db.collection("cash_categories")
}
