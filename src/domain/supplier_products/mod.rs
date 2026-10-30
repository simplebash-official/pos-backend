// Pure business types for the supplier-product linking feature (the
// many-to-many join between `suppliers` and `inventory`'s products) — no
// I/O, no Mongo/Axum types beyond serde/utoipa derives.

mod bulk_replace_links_request;
mod supplier_product_link;
mod supplier_product_link_query;
mod supplier_product_links_response;
mod supplier_product_list_response;
mod upsert_supplier_product_link_request;

pub use bulk_replace_links_request::BulkReplaceLinksRequest;
pub use supplier_product_link::SupplierProductLink;
pub use supplier_product_link_query::SupplierProductLinkQuery;
pub use supplier_product_links_response::SupplierProductLinksResponse;
pub use supplier_product_list_response::SupplierProductListResponse;
pub use upsert_supplier_product_link_request::UpsertSupplierProductLinkRequest;
