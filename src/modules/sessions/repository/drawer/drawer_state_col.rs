use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::sessions::model::DrawerStateDocument;

pub(super) fn drawer_state_col(db: &TenantDatabase) -> ScopedCollection<DrawerStateDocument> {
    db.collection("drawer_state")
}
