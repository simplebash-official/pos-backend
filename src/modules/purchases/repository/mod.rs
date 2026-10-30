// Mongo access for the `purchases` collection only. Functions here never
// interpret a missing document as an error — they return `Vec`/counts
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can
// call in, but the `mod repository;` declaration in `purchases/mod.rs` is
// private, so none of this is reachable from outside this module tree.

pub(crate) mod charge;
mod count_purchases_for_supplier;
pub(crate) mod credit;
mod insert_purchase;
mod list_purchases_by_product;
mod list_purchases_by_receipts;
mod list_purchases_by_supplier;
mod list_purchases_paginated;
mod purchases;
pub(crate) mod receipt;
mod row_to_purchase_doc;
pub(crate) mod settlement;

pub(crate) use count_purchases_for_supplier::count_purchases_for_supplier;
pub(crate) use insert_purchase::insert_purchase;

pub(crate) use list_purchases_by_receipts::list_purchases_by_receipts;
pub(crate) use list_purchases_by_supplier::list_purchases_by_supplier;
pub(crate) use list_purchases_paginated::list_purchases_paginated;
