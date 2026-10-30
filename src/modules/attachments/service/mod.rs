//! Business rules for receipt photos: what can be attached, to what, and how many. The image is
//! checked by its bytes, not by the name or type the client claims.

mod attachments_service;
#[cfg(test)]
mod tests;

pub use attachments_service::AttachmentContent;
pub use attachments_service::MAX_BYTES;
pub use attachments_service::get_content;
pub use attachments_service::list_for;
#[cfg(test)]
use attachments_service::sniff;
pub use attachments_service::upload;
