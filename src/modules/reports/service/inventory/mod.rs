//! Inventory valuation and top-product reports.

mod service_inventory;

pub(crate) use service_inventory::get_inventory_valuation;
pub(crate) use service_inventory::get_top_products;
