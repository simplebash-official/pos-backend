use super::add_subcategory;
use super::adjust_stock;
use super::create_category;
use super::create_product;
use super::delete_category;
use super::delete_product;
use super::delete_products;
use super::generate_product_barcode;
use super::get_category_subcategories;
use super::get_product;
use super::get_product_by_barcode;
use super::get_serial_by_number;
use super::get_valid_categories;
use super::inventory_overview;
use super::inventory_stats;
use super::list_all_stock_movements;
use super::list_categories;
use super::list_products;
use super::low_stock;
use super::product_batches;
use super::product_movements;
use super::product_serials;
use super::remove_subcategory;
use super::status;
use super::update_category;
use super::update_product;
use super::update_stock_batch;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    // Each `routes!()` call registers one axum path — handlers only share a
    // call when they're different HTTP methods on the *same* path (e.g.
    // GET/PUT/DELETE all on "/products/{id}"). Mixing handlers with
    // different paths into one call would fold them onto a single
    // `MethodRouter` and panic on the first repeated method.
    OpenApiRouter::new()
        // Module
        .routes(routes!(status::status))
        // Overview
        .routes(routes!(inventory_overview::inventory_overview))
        // Stats
        .routes(routes!(inventory_stats::inventory_stats))
        // Products
        .routes(routes!(
            list_products::list_products,
            create_product::create_product,
            delete_products::delete_products
        ))
        .routes(routes!(get_product_by_barcode::get_product_by_barcode))
        .routes(routes!(generate_product_barcode::generate_product_barcode))
        .routes(routes!(
            get_product::get_product,
            update_product::update_product,
            delete_product::delete_product
        ))
        // Stock
        .routes(routes!(list_all_stock_movements::list_all_stock_movements))
        .routes(routes!(adjust_stock::adjust_stock))
        .routes(routes!(low_stock::low_stock))
        .routes(routes!(product_movements::product_movements))
        .routes(routes!(product_serials::product_serials))
        .routes(routes!(get_serial_by_number::get_serial_by_number))
        .routes(routes!(product_batches::product_batches))
        .routes(routes!(update_stock_batch::update_stock_batch))
        // Categories
        .routes(routes!(
            list_categories::list_categories,
            create_category::create_category
        ))
        .routes(routes!(
            update_category::update_category,
            delete_category::delete_category
        ))
        .routes(routes!(get_valid_categories::get_valid_categories))
        // Subcategories
        .routes(routes!(
            get_category_subcategories::get_category_subcategories,
            add_subcategory::add_subcategory
        ))
        .routes(routes!(remove_subcategory::remove_subcategory))
}
