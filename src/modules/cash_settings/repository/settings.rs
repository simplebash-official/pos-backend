use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::cash_settings::model::CashSettingsDocument;

pub(super) fn settings(db: &TenantDatabase) -> ScopedCollection<CashSettingsDocument> {
    db.collection("cash_settings")
}
