// Business rules for product CRUD/listing: price/stock invariants, SKU
// uniqueness, and category/subcategory validation against the
// `categories`/`subcategories` collections. Delegates all Mongo access to
// `repository::product`/`repository::category`/`repository::subcategory`.

mod create_product;
mod delete_product;
mod delete_products;
mod ensure_barcode_available;
mod ensure_valid_category;
mod generate_barcode;
mod get_product;
mod get_product_by_barcode;
mod get_product_by_key;
mod get_products_by_keys;
mod hydrate_sync_documents;
mod list_products;
mod list_sort_for;
mod resolve_display_names;
mod sort_field_for;
mod update_product;
mod validate_manual_barcode;
mod validate_min_stock_threshold;

pub use create_product::create_product;
pub(crate) use delete_product::delete_product;
pub(crate) use delete_products::delete_products;
pub(crate) use generate_barcode::generate_barcode;
pub(crate) use get_product::get_product;
pub(crate) use get_product_by_barcode::get_product_by_barcode;
pub(crate) use get_product_by_key::get_product_by_key;
pub(crate) use get_products_by_keys::get_products_by_keys;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_products::list_products;

pub(crate) use sort_field_for::sort_field_for;
pub(crate) use update_product::update_product;
