// Mongo access for the `product_serials` collection only — one document
// per physical serialized unit. Never updated except by
// `service::product_serial`'s status-transition helpers; deletion is not
// supported (a unit's history is kept forever, same rationale as
// `stock_movements`).

mod find_serial_by_number;
mod find_serial_by_number_and_product;
mod find_serials_for_product;
mod insert_product_serial;
mod product_serials;
mod row_to_serial_doc;
mod update_serial;

pub(crate) use find_serial_by_number::find_serial_by_number;
pub(crate) use find_serial_by_number_and_product::find_serial_by_number_and_product;
pub(crate) use find_serials_for_product::find_serials_for_product;
pub(crate) use insert_product_serial::insert_product_serial;
pub(crate) use update_serial::update_serial;
