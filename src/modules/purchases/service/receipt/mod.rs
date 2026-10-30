//! Business rules for supplier receipts: one invoice's worth of stock received at once. A receipt
//! is validated and its supplier and products resolved before anything is written; then the
//! header, every line, every line's stock batches (each with its `PurchaseReceipt` movement), the
//! serials, the extra charges and every payment are written in one transaction, so a failure
//! leaves no half-received invoice behind. The units come in through
//! `inventory::service::stock_batch::create_batch`, the one place stock is ever created.
//!
//! A line is priced by its total (the supplier's lot price). Extra charges (shipping, duty,
//! handling, insurance, a discount) are spread over the lines in cents (`charges::allocate`), and
//! a line's landed total (its price plus its share) becomes one or two batches that add up to it
//! exactly (`charges::split_cost`). Only the charges the supplier billed change what is owed to
//! the supplier; the ones paid to someone else are paid now, from the chosen source.

mod record_receipt;
mod service_receipt;

pub use record_receipt::record_receipt;
pub use service_receipt::RecordedBy;
pub use service_receipt::get_receipt;
pub(crate) use service_receipt::hydrate_charge_documents;
pub(crate) use service_receipt::hydrate_sync_documents;
pub use service_receipt::list_receipts;
