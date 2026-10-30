//! Dual-engine (Mongo / SQLite) access for the `supplier_products` collection.
//! Functions here return `Option`/`Vec`/counts straight from the driver.

mod supplier_products_repository;

pub(crate) use supplier_products_repository::delete_link;
pub(crate) use supplier_products_repository::delete_links_by_product;
pub(crate) use supplier_products_repository::delete_links_by_supplier;
pub(crate) use supplier_products_repository::find_link;
pub(crate) use supplier_products_repository::insert_link;

pub(crate) use supplier_products_repository::list_links_by_supplier;
pub(crate) use supplier_products_repository::list_links_paginated;
pub(crate) use supplier_products_repository::update_link;
