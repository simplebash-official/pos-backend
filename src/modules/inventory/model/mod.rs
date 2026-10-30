// Mongo document shapes for the inventory feature. Kept separate from
// `domain::inventory` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

mod category_document;
mod default_version;
mod product_document;
mod product_serial_document;
mod stock_batch_document;
mod stock_movement_document;
mod subcategory_document;

pub use category_document::CategoryDocument;
pub use product_document::ProductDocument;
pub use product_serial_document::ProductSerialDocument;
pub use stock_batch_document::StockBatchDocument;
pub use stock_movement_document::StockMovementDocument;
pub use subcategory_document::SubcategoryDocument;
