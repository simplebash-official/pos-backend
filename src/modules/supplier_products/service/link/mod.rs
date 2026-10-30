// Business rules for supplier-product linking: validating that both sides
// of a link actually exist (delegating to `inventory::service::product` and
// `suppliers::service` — the two collections this module joins), the
// upsert-not-duplicate behavior on create, and the two cascade-delete entry
// points other modules call when a supplier or product is removed.

mod delete_link;
mod delete_links_for_product;
mod delete_links_for_supplier;
mod hydrate_sync_documents;
mod list_links;
mod replace_links_for_supplier;
mod upsert_link;

pub(crate) use delete_link::delete_link;
pub(crate) use delete_links_for_product::delete_links_for_product;
pub(crate) use delete_links_for_supplier::delete_links_for_supplier;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_links::list_links;
pub(crate) use replace_links_for_supplier::replace_links_for_supplier;
pub use upsert_link::upsert_link;
