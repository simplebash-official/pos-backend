// Business rules for supplier credits: an amount a supplier takes off what the shop owes, for
// short or damaged goods or a price difference. A credit against a receipt can never be more than
// that receipt still owes. Credits move no stock. A wrong credit is voided by a reversing entry
// whose key is derived from the credit's, so two devices voiding the same credit offline write
// the same row; nothing is ever edited or deleted.

mod create_credit;
mod credited_by_receipt;
mod credits_of_supplier;
mod hydrate_sync_documents;
mod list_credits;
mod live_credits;
mod void_credit;

pub use create_credit::create_credit;
pub(crate) use credited_by_receipt::credited_by_receipt;
pub(crate) use credits_of_supplier::credits_of_supplier;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_credits::list_credits;
pub use void_credit::void_credit;
