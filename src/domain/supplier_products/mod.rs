//! Pure business types for the supplier-product linking feature (the
//! many-to-many join between `suppliers` and `inventory`'s products) — no
//! I/O, no Mongo/Axum types beyond serde/utoipa derives.

mod domain_supplier_products;

pub use domain_supplier_products::BulkReplaceLinksRequest;
pub use domain_supplier_products::SupplierProductLink;
pub use domain_supplier_products::SupplierProductLinkQuery;
pub use domain_supplier_products::SupplierProductLinksResponse;
pub use domain_supplier_products::SupplierProductListResponse;
pub use domain_supplier_products::UpsertSupplierProductLinkRequest;
