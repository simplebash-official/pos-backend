//! Business rules for stock changes: the adjustment invariant (never let a
//! batch's or a product's stock go negative), and the fact that every
//! adjustment must also produce a `StockMovement` audit record — the batch,
//! the product total and the movement are always written together from here,
//! never independently.

mod service_stock;

pub(crate) use service_stock::adjust_stock;
pub(crate) use service_stock::apply_stock_delta;
pub(crate) use service_stock::hydrate_sync_documents;
pub(crate) use service_stock::list_stock_movements;
pub(crate) use service_stock::low_stock;
pub(crate) use service_stock::product_movements;
