// Mongo document shape for the supplier-product linking feature. Kept
// separate from `domain::supplier_products` (the API-facing types) so BSON
// concerns like `ObjectId` never leak into request/response payloads.

mod supplier_product_link_document;

pub use supplier_product_link_document::SupplierProductLinkDocument;
