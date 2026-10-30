//! Business rules for individually-serialized inventory units: minting
//! serials on a purchase receipt, resolving/consuming one at sale time, and
//! transitioning a unit's status when it comes back on a credit note. Called
//! into from `purchases`/`billing` (sibling top-level modules) via
//! `pub(crate)` functions here, the same "reach the other module through its
//! service, never its repository" rule every other cross-module dependency
//! in this codebase follows.

mod service_product_serial;

pub(crate) use service_product_serial::create_serials_for_purchase;
pub(crate) use service_product_serial::ensure_serials_untracked;
pub(crate) use service_product_serial::find_serial_id;
pub(crate) use service_product_serial::hydrate_sync_documents;
pub(crate) use service_product_serial::is_within_warranty;
pub(crate) use service_product_serial::list_serials_for_product;
pub(crate) use service_product_serial::lookup_in_stock_serial;
pub(crate) use service_product_serial::mark_serial_sold;
pub(crate) use service_product_serial::resolve_in_stock_serial;
pub(crate) use service_product_serial::resolve_sold_serial_for_invoice;
pub(crate) use service_product_serial::transition_serial_on_return;
