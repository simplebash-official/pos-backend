//! Mongo access for the `product_serials` collection only — one document
//! per physical serialized unit. Never updated except by
//! `service::product_serial`'s status-transition helpers; deletion is not
//! supported (a unit's history is kept forever, same rationale as
//! `stock_movements`).

mod repository_product_serial;

pub(crate) use repository_product_serial::find_serial_by_number;
pub(crate) use repository_product_serial::find_serial_by_number_and_product;
pub(crate) use repository_product_serial::find_serials_for_product;
pub(crate) use repository_product_serial::insert_product_serial;
pub(crate) use repository_product_serial::update_serial;
