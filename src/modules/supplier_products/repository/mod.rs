// Dual-engine (Mongo / SQLite) access for the `supplier_products` collection.
// Functions here return `Option`/`Vec`/counts straight from the driver.

mod delete_link;
mod delete_links_by_product;
mod delete_links_by_supplier;
mod delete_links_where;
mod find_link;
mod insert_link;
mod list_links_by_product;
mod list_links_by_supplier;
mod list_links_paginated;
mod record_link_deletes;
mod supplier_product_link_sqlite_row;
mod supplier_products;
mod update_link;

pub(crate) use delete_link::delete_link;
pub(crate) use delete_links_by_product::delete_links_by_product;
pub(crate) use delete_links_by_supplier::delete_links_by_supplier;
pub(crate) use find_link::find_link;
pub(crate) use insert_link::insert_link;

pub(crate) use list_links_by_supplier::list_links_by_supplier;
pub(crate) use list_links_paginated::list_links_paginated;
pub(crate) use update_link::update_link;
