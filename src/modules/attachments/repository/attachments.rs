use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::modules::attachments::model::AttachmentDocument;

pub(super) fn attachments(db: &TenantDatabase) -> ScopedCollection<AttachmentDocument> {
    db.collection("attachments")
}
