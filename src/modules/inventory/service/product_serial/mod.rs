// Business rules for individually-serialized inventory units: minting
// serials on a purchase receipt, resolving/consuming one at sale time, and
// transitioning a unit's status when it comes back on a credit note. Called
// into from `purchases`/`billing` (sibling top-level modules) via
// `pub(crate)` functions here, the same "reach the other module through its
// service, never its repository" rule every other cross-module dependency
// in this codebase follows.

mod create_serials_for_purchase;
mod ensure_serials_untracked;
mod find_serial_id;
mod hydrate_sync_documents;
mod is_within_warranty;
mod list_serials_for_product;
mod lookup_in_stock_serial;
mod mark_serial_sold;
mod resolve_in_stock_serial;
mod resolve_sold_serial_for_invoice;
mod transition_serial_on_return;

pub(crate) use create_serials_for_purchase::create_serials_for_purchase;
pub(crate) use ensure_serials_untracked::ensure_serials_untracked;
pub(crate) use find_serial_id::find_serial_id;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub(crate) use is_within_warranty::is_within_warranty;
pub(crate) use list_serials_for_product::list_serials_for_product;
pub(crate) use lookup_in_stock_serial::lookup_in_stock_serial;
pub(crate) use mark_serial_sold::mark_serial_sold;
pub(crate) use resolve_in_stock_serial::resolve_in_stock_serial;
pub(crate) use resolve_sold_serial_for_invoice::resolve_sold_serial_for_invoice;
pub(crate) use transition_serial_on_return::transition_serial_on_return;
