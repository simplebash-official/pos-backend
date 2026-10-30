//! Business rules for supplier-product linking: validating that both sides
//! of a link actually exist (delegating to `inventory::service::product` and
//! `suppliers::service` — the two collections this module joins), the
//! upsert-not-duplicate behavior on create, and the two cascade-delete entry
//! points other modules call when a supplier or product is removed.

mod service_link;

pub(crate) use service_link::delete_link;
pub(crate) use service_link::delete_links_for_product;
pub(crate) use service_link::delete_links_for_supplier;
pub(crate) use service_link::hydrate_sync_documents;
pub use service_link::list_links;
pub(crate) use service_link::replace_links_for_supplier;
pub use service_link::upsert_link;
