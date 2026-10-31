//! Product storage and queries, on MongoDB and SQLite.

mod product_listing;
mod product_lookups;
mod product_overview;
mod product_shared;
mod product_writes;
#[cfg(test)]
mod tests;

pub(crate) use product_listing::list_products_with_display_names;
pub(crate) use product_lookups::count_products_in_category;
pub(crate) use product_lookups::count_products_in_subcategory;
pub(crate) use product_lookups::count_stats;
pub(crate) use product_lookups::find_low_stock_products;
pub(crate) use product_lookups::find_product_by_barcode;
pub(crate) use product_lookups::find_product_by_barcode_excluding;
pub(crate) use product_lookups::find_product_by_id;
pub(crate) use product_lookups::find_product_by_key;
pub(crate) use product_lookups::find_product_by_sku;
pub(crate) use product_lookups::find_products_by_ids;
pub(crate) use product_overview::aggregate_overview;
pub(crate) use product_shared::MIN_SELLING_PRICE;
pub(crate) use product_writes::adjust_product_stock_pipeline;
pub(crate) use product_writes::delete_product;
pub(crate) use product_writes::delete_products;
pub(crate) use product_writes::insert_product;

pub(crate) use product_writes::update_product;
