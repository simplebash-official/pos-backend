use crate::modules::attachments::model::AttachmentDocument;

/// One photo's stored record together with its decoded image bytes.
#[derive(Debug)]
pub struct AttachmentContent {
    pub document: AttachmentDocument,
    pub bytes: Vec<u8>,
}
