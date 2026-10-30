//! Database access for the `stock_batches` collection only — one row per
//! delivery of a product, with its own cost, selling price and remaining
//! quantity. `remaining_quantity` is a derived column (the sum of the stock
//! movements carrying the batch's key): it only moves through
//! `adjust_batch_remaining`, which `service::stock::apply_stock_delta` calls
//! together with the movement insert, and never bumps `version`/`updated_at`
//! (same rule as `products.stock_quantity`).

mod repository_stock_batch;

pub(crate) use repository_stock_batch::adjust_batch_remaining;
pub(crate) use repository_stock_batch::find_batch_by_key;
pub(crate) use repository_stock_batch::insert_batch;
pub(crate) use repository_stock_batch::list_batches_for_product;
pub(crate) use repository_stock_batch::list_in_stock_batches_for_products;
pub(crate) use repository_stock_batch::list_latest_batches_for_products;
pub(crate) use repository_stock_batch::update_batch;
