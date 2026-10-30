// Business rules for stock changes: the adjustment invariant (never let a
// batch's or a product's stock go negative), and the fact that every
// adjustment must also produce a `StockMovement` audit record — the batch,
// the product total and the movement are always written together from here,
// never independently.

mod adjust_stock;
mod apply_stock_delta;
mod hydrate_sync_documents;
mod list_stock_movements;
mod low_stock;
mod product_movements;

pub(crate) use adjust_stock::adjust_stock;
pub(crate) use apply_stock_delta::apply_stock_delta;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub(crate) use list_stock_movements::list_stock_movements;
pub(crate) use low_stock::low_stock;
pub(crate) use product_movements::product_movements;
