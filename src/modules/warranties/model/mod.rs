//! How warranties, their activity log and their claims are stored.

mod claim_document;
mod warranty_document;

pub use claim_document::WarrantyClaimDocument;
pub use warranty_document::WarrantyDocument;
pub use warranty_document::WarrantyEventDocument;
pub(crate) use warranty_document::stored_day;
