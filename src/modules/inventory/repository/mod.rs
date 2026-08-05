// Mongo-only query/persistence layer for the inventory module. Functions
// here never interpret a missing document as an error — they return
// `Option`/`Vec`/counts straight from the driver and leave the "not found"
// -> `AppError` translation to `service`. Visibility is `pub(crate)` so
// `service` (a sibling module under `inventory`) can call in, but the
// `mod repository;` declaration in `inventory/mod.rs` is private, so none
// of this is reachable from outside the `inventory` module tree.

pub(crate) mod category;
pub(crate) mod product;
pub(crate) mod sku_counter;
pub(crate) mod stock;
pub(crate) mod subcategory;
