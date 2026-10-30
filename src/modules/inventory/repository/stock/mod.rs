//! Mongo access for the `stock_movements` collection only — the append-only
//! audit trail behind every stock change. Movements are never updated or
//! deleted here, only inserted and read.

mod repository_stock;

pub(crate) use repository_stock::find_stock_movements_for_product;
pub(crate) use repository_stock::insert_stock_movement;
pub(crate) use repository_stock::list_stock_movements_paginated;
