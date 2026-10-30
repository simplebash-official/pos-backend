// Business rules for receipt photos: what can be attached, to what, and how many. The image is
// checked by its bytes, not by the name or type the client claims.

mod attachment_content;
mod dto;
mod get_content;
mod list_for;
mod max_bytes;
mod sniff;
#[cfg(test)]
mod tests;
mod upload;

pub use attachment_content::AttachmentContent;
pub use get_content::get_content;
pub use list_for::list_for;
pub use max_bytes::MAX_BYTES;
#[cfg(test)]
use sniff::sniff;
pub use upload::upload;
