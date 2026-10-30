//! Business rules for supplier credits: an amount a supplier takes off what the shop owes, for
//! short or damaged goods or a price difference. A credit against a receipt can never be more than
//! that receipt still owes. Credits move no stock. A wrong credit is voided by a reversing entry
//! whose key is derived from the credit's, so two devices voiding the same credit offline write
//! the same row; nothing is ever edited or deleted.

mod service_credit;

pub use service_credit::create_credit;
pub(crate) use service_credit::credited_by_receipt;
pub(crate) use service_credit::credits_of_supplier;
pub(crate) use service_credit::hydrate_sync_documents;
pub use service_credit::list_credits;
pub use service_credit::void_credit;
