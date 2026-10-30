//! Mongo access for the `purchases` collection only. Functions here never
//! interpret a missing document as an error — they return `Vec`/counts
//! straight from the driver and leave the "not found" -> `AppError`
//! translation to `service`. Visibility is `pub(crate)` so `service` can
//! call in, but the `mod repository;` declaration in `purchases/mod.rs` is
//! private, so none of this is reachable from outside this module tree.

pub(crate) mod charge;
pub(crate) mod credit;
mod purchases_repository;
pub(crate) mod receipt;
pub(crate) mod settlement;

pub(crate) use purchases_repository::count_purchases_for_supplier;
pub(crate) use purchases_repository::insert_purchase;

pub(crate) use purchases_repository::list_purchases_by_receipts;
pub(crate) use purchases_repository::list_purchases_by_supplier;
pub(crate) use purchases_repository::list_purchases_paginated;
