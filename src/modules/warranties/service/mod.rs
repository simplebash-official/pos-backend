//! Business rules for warranties: issuing them when a sale is completed, cancelling them when
//! the sale is voided or the item comes back, finding them from whatever staff scan or type,
//! and the manager-approved void and extend, and claims from the counter to the supplier and back. Other modules reach these through the
//! `pub(crate)` functions below, never the repository.

mod claim_outcomes;
mod claim_steps;
mod warranty_events;
mod warranty_issue;
mod warranty_lifecycle;
mod warranty_lookup;

pub(crate) use claim_outcomes::check_claim_refund;
pub(crate) use claim_outcomes::convert_to_paid_repair;
pub(crate) use claim_outcomes::finish_claim_refund;
pub(crate) use claim_outcomes::open_rework;
pub(crate) use claim_outcomes::resolve_claim;
pub(crate) use claim_steps::decide_claim;
pub(crate) use claim_steps::get_claim;
pub(crate) use claim_steps::hydrate_sync_claims;
pub(crate) use claim_steps::inspect_claim;
pub(crate) use claim_steps::list_claims;
pub(crate) use claim_steps::open_claim;
pub(crate) use claim_steps::return_to_customer;
pub(crate) use claim_steps::send_to_supplier;
pub(crate) use claim_steps::supplier_return;
pub(crate) use warranty_events::WarrantyActor;
pub(crate) use warranty_issue::RepairLine;
pub(crate) use warranty_issue::SaleLine;
pub(crate) use warranty_issue::issue_for_repair;
pub(crate) use warranty_issue::issue_for_sale_line;
pub(crate) use warranty_issue::phone_tail;
pub(crate) use warranty_issue::serial_within_warranty;
pub(crate) use warranty_issue::void_for_invoice;
pub(crate) use warranty_issue::void_for_returned_line;
pub(crate) use warranty_issue::void_for_returned_serial;
pub(crate) use warranty_lifecycle::extend;
pub(crate) use warranty_lifecycle::void;
pub(crate) use warranty_lookup::detail;
pub(crate) use warranty_lookup::for_invoice_print;
pub(crate) use warranty_lookup::hydrate_sync_documents;
pub(crate) use warranty_lookup::hydrate_sync_events;
pub(crate) use warranty_lookup::list;
pub(crate) use warranty_lookup::lookup;
