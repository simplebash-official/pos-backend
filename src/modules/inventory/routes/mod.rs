// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there.

mod add_subcategory;
mod adjust_stock;
mod batch_for;
mod create_category;
mod create_product;
mod delete_category;
mod delete_product;
mod delete_products;
mod generate_product_barcode;
mod get_category_subcategories;
mod get_product;
mod get_product_by_barcode;
mod get_serial_by_number;
mod get_valid_categories;
mod inventory_overview;
mod inventory_stats;
mod list_all_stock_movements;
mod list_categories;
mod list_products;
mod low_stock;
mod parse_object_id;
mod product_batches;
mod product_for;
mod product_movements;
mod product_serials;
mod remove_subcategory;
mod router;
mod status;
mod update_category;
mod update_product;
mod update_stock_batch;

pub use router::router;
