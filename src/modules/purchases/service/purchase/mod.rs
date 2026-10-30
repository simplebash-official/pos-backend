//! Business rules for reading supplier purchase lines. A line is created only as part of a
//! receipt (`super::receipt`), which resolves both sides (supplier, product) before anything is
//! written and brings the units in as a new stock batch. This file lists and enriches lines.

mod service_purchase;

pub(crate) use service_purchase::costs_of_supplier;
pub(crate) use service_purchase::count_purchases_for_supplier;
pub(crate) use service_purchase::enrich_purchases;
pub(crate) use service_purchase::hydrate_sync_documents;
pub use service_purchase::list_purchases;
