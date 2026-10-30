use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::auth::model::LoginSessionDocument;

pub(super) fn login_sessions(db: &TenantDatabase) -> ScopedCollection<LoginSessionDocument> {
    db.collection("login_sessions")
}
