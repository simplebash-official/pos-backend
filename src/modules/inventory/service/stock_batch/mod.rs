//! Business rules for stock batches — one per delivery of a product, each
//! with its own cost and selling price. Owns: creating a batch (always
//! together with the movement that brings its units in), grouping in-stock
//! batches into the price tiers a cashier picks from, planning which batches a
//! sale of N units at a tier draws on (oldest first), restocking returned
//! units, and batch price edits. Stock quantities themselves only ever move
//! through `service::stock::apply_stock_delta`.

mod service_stock_batch;
#[cfg(test)]
mod tests;

pub(crate) use service_stock_batch::allocations_for_return;
pub(crate) use service_stock_batch::allocations_for_serials;
pub(crate) use service_stock_batch::attach_price_tiers;
pub use service_stock_batch::backfill_line_allocations;
pub(crate) use service_stock_batch::batch_not_found;
pub(crate) use service_stock_batch::create_batch;
pub(crate) use service_stock_batch::get_batch_for_product;
pub(crate) use service_stock_batch::hydrate_sync_documents;

pub(crate) use service_stock_batch::NewBatch;
pub(crate) use service_stock_batch::in_stock_units;
pub(crate) use service_stock_batch::list_batches;
pub(crate) use service_stock_batch::move_allocations;
pub(crate) use service_stock_batch::newest_batch_at_price;

pub(crate) use service_stock_batch::plan_tier_allocation;

pub(crate) use service_stock_batch::update_batch;
pub(crate) use service_stock_batch::validate_batch_prices;
pub(crate) use service_stock_batch::weighted_unit_cost;
pub(crate) use service_stock_batch::with_price_tiers;
