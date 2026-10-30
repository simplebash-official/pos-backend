//! Pure business types for receipt photos attached to a record. No I/O, no framework types beyond
//! serde/utoipa derives.

mod domain_attachments;

pub use domain_attachments::Attachment;
pub use domain_attachments::AttachmentListQuery;
pub use domain_attachments::AttachmentOwner;
pub use domain_attachments::UploadAttachmentRequest;
