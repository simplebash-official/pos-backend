//! Business rules for product CRUD/listing: price/stock invariants, SKU
//! uniqueness, and category/subcategory validation against the
//! `categories`/`subcategories` collections. Delegates all Mongo access to
//! `repository::product`/`repository::category`/`repository::subcategory`.

mod service_product;

pub use service_product::create_product;
pub(crate) use service_product::delete_product;
pub(crate) use service_product::delete_products;
pub(crate) use service_product::generate_barcode;
pub(crate) use service_product::get_product;
pub(crate) use service_product::get_product_by_barcode;
pub(crate) use service_product::get_product_by_key;
pub(crate) use service_product::get_products_by_keys;
pub(crate) use service_product::hydrate_sync_documents;
pub use service_product::list_products;

pub(crate) use service_product::sort_field_for;
pub(crate) use service_product::update_product;
