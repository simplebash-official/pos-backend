use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::sessions::model::RegisterSessionDocument;

pub(super) fn sessions_col(db: &TenantDatabase) -> ScopedCollection<RegisterSessionDocument> {
    db.collection("register_sessions")
}
