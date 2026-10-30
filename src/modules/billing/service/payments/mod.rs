// Standalone payment recording — the server-side twin of the frontend's
// already-designed-but-unwired `paymentsStore.ts` (`recordPayment`/
// `deriveInvoiceStatus`). This is how a credit invoice gets paid off over
// time, in one or more installments, independent of `sale::complete_sale`.

mod hydrate_sync_documents;
mod list_payments;
mod record_payment;
mod totals_by_method_since;

pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_payments::list_payments;
pub use record_payment::record_payment;
pub(crate) use totals_by_method_since::totals_by_method_since;
