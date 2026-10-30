//! Product storage and queries, on MongoDB and SQLite.

mod repository_product_part1;
mod repository_product_part2;
#[cfg(test)]
mod tests;

pub(crate) use repository_product_part1::adjust_product_stock_pipeline;
pub(crate) use repository_product_part1::aggregate_overview;
pub(crate) use repository_product_part1::count_products_in_category;
pub(crate) use repository_product_part1::count_products_in_subcategory;
pub(crate) use repository_product_part1::count_stats;
pub(crate) use repository_product_part1::delete_product;
pub(crate) use repository_product_part1::delete_products;
pub(crate) use repository_product_part1::find_low_stock_products;
pub(crate) use repository_product_part1::find_product_by_barcode;
pub(crate) use repository_product_part1::find_product_by_barcode_excluding;
pub(crate) use repository_product_part1::find_product_by_id;
pub(crate) use repository_product_part1::find_product_by_key;
pub(crate) use repository_product_part1::find_product_by_sku;
pub(crate) use repository_product_part1::find_products_by_ids;
pub(crate) use repository_product_part1::insert_product;
pub(crate) use repository_product_part2::MIN_SELLING_PRICE;
pub(crate) use repository_product_part2::list_products_with_display_names;

pub(crate) use repository_product_part2::update_product;
