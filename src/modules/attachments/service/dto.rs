use crate::domain::attachments::Attachment;
use crate::modules::attachments::model::AttachmentDocument;

pub(super) fn dto(d: &AttachmentDocument) -> Attachment {
    Attachment {
        key: d.key.clone(),
        owner_kind: d.owner_kind,
        owner_key: d.owner_key.clone(),
        file_name: d.file_name.clone(),
        content_type: d.content_type.clone(),
        size_bytes: d.size_bytes,
        uploaded_by_name_snapshot: d.uploaded_by_name_snapshot.clone(),
        created_at: d.created_at.to_chrono(),
    }
}
