// Mongo access for the `stock_movements` collection only — the append-only
// audit trail behind every stock change. Movements are never updated or
// deleted here, only inserted and read.

mod find_stock_movements_for_product;
mod insert_stock_movement;
mod list_stock_movements_paginated;
mod row_to_stock_movement_doc;
mod stock_movements;

pub(crate) use find_stock_movements_for_product::find_stock_movements_for_product;
pub(crate) use insert_stock_movement::insert_stock_movement;
pub(crate) use list_stock_movements_paginated::list_stock_movements_paginated;
