// Database access for the `stock_batches` collection only — one row per
// delivery of a product, with its own cost, selling price and remaining
// quantity. `remaining_quantity` is a derived column (the sum of the stock
// movements carrying the batch's key): it only moves through
// `adjust_batch_remaining`, which `service::stock::apply_stock_delta` calls
// together with the movement insert, and never bumps `version`/`updated_at`
// (same rule as `products.stock_quantity`).

mod adjust_batch_remaining;
mod find_batch_by_key;
mod insert_batch;
mod list_batches_for_product;
mod list_in_stock_batches_for_products;
mod list_latest_batches_for_products;
mod row_to_batch_doc;
mod stock_batches;
mod update_batch;

pub(crate) use adjust_batch_remaining::adjust_batch_remaining;
pub(crate) use find_batch_by_key::find_batch_by_key;
pub(crate) use insert_batch::insert_batch;
pub(crate) use list_batches_for_product::list_batches_for_product;
pub(crate) use list_in_stock_batches_for_products::list_in_stock_batches_for_products;
pub(crate) use list_latest_batches_for_products::list_latest_batches_for_products;
pub(crate) use update_batch::update_batch;
