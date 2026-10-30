// HTTP layer for receipt photos. Who may see or add a photo follows the record it belongs to.

mod get_attachment_content;
mod list_attachments;
mod require_owner_access;
mod router;
mod upload_attachment;

pub use router::router;
