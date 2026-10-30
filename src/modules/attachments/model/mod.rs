//! Mongo document shape for a receipt photo, kept apart from the API-facing `domain::attachments`
//! so BSON types never leak into payloads.

mod attachment_document;

pub use attachment_document::AttachmentDocument;
