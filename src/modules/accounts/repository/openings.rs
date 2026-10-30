use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::accounts::model::AccountOpeningDocument;

pub(super) fn openings(db: &TenantDatabase) -> ScopedCollection<AccountOpeningDocument> {
    db.collection("account_openings")
}
