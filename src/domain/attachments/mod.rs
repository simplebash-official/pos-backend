// Pure business types for receipt photos attached to a record. No I/O, no framework types beyond
// serde/utoipa derives.

mod attachment;
mod attachment_list_query;
mod attachment_owner;
mod upload_attachment_request;

pub use attachment::Attachment;
pub use attachment_list_query::AttachmentListQuery;
pub use attachment_owner::AttachmentOwner;
pub use upload_attachment_request::UploadAttachmentRequest;
