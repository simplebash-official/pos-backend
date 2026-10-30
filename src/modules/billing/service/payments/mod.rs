//! Standalone payment recording — the server-side twin of the frontend's
//! already-designed-but-unwired `paymentsStore.ts` (`recordPayment`/
//! `deriveInvoiceStatus`). This is how a credit invoice gets paid off over
//! time, in one or more installments, independent of `sale::complete_sale`.

mod service_payments;

pub(crate) use service_payments::hydrate_sync_documents;
pub use service_payments::list_payments;
pub use service_payments::record_payment;
pub(crate) use service_payments::totals_by_method_since;
