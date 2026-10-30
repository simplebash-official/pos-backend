// Business rules for reading supplier purchase lines. A line is created only as part of a
// receipt (`super::receipt`), which resolves both sides (supplier, product) before anything is
// written and brings the units in as a new stock batch. This file lists and enriches lines.

mod costs_of_supplier;
mod count_purchases_for_supplier;
mod enrich_purchases;
mod hydrate_sync_documents;
mod list_purchases;

pub(crate) use costs_of_supplier::costs_of_supplier;
pub(crate) use count_purchases_for_supplier::count_purchases_for_supplier;
pub(crate) use enrich_purchases::enrich_purchases;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_purchases::list_purchases;
