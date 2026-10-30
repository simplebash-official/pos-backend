use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};

use crate::modules::users::model::UserDocument;

pub(super) fn users(db: &TenantDatabase) -> ScopedCollection<UserDocument> {
    db.collection("users")
}
