// Business rules for stock batches — one per delivery of a product, each
// with its own cost and selling price. Owns: creating a batch (always
// together with the movement that brings its units in), grouping in-stock
// batches into the price tiers a cashier picks from, planning which batches a
// sale of N units at a tier draws on (oldest first), restocking returned
// units, and batch price edits. Stock quantities themselves only ever move
// through `service::stock::apply_stock_delta`.

mod allocations_for_return;
mod allocations_for_serials;
mod attach_price_tiers;
mod backfill_line_allocations;
mod batch_not_found;
mod create_batch;
mod get_batch_for_product;
mod hydrate_sync_documents;
mod in_stock_units;
mod in_stock_units_fn;
mod list_batches;
mod move_allocations;
mod new_batch;
mod newest_batch_at_price;
mod plan_from_batches;
mod plan_tier_allocation;
#[cfg(test)]
mod tests;
mod tiers_from_batches;
mod update_batch;
mod validate_batch_prices;
mod weighted_unit_cost;
mod with_price_tiers;

pub(crate) use allocations_for_return::allocations_for_return;
pub(crate) use allocations_for_serials::allocations_for_serials;
pub(crate) use attach_price_tiers::attach_price_tiers;
pub use backfill_line_allocations::backfill_line_allocations;
pub(crate) use batch_not_found::batch_not_found;
pub(crate) use create_batch::create_batch;
pub(crate) use get_batch_for_product::get_batch_for_product;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;

pub(crate) use in_stock_units_fn::in_stock_units;
pub(crate) use list_batches::list_batches;
pub(crate) use move_allocations::move_allocations;
pub(crate) use new_batch::NewBatch;
pub(crate) use newest_batch_at_price::newest_batch_at_price;

pub(crate) use plan_tier_allocation::plan_tier_allocation;

pub(crate) use update_batch::update_batch;
pub(crate) use validate_batch_prices::validate_batch_prices;
pub(crate) use weighted_unit_cost::weighted_unit_cost;
pub(crate) use with_price_tiers::with_price_tiers;
