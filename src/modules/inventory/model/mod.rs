//! Mongo document shapes for the inventory feature. Kept separate from
//! `domain::inventory` (the API-facing types) so BSON concerns like
//! `ObjectId` never leak into request/response payloads.

mod inventory_model;

pub use inventory_model::CategoryDocument;
pub use inventory_model::ProductDocument;
pub use inventory_model::ProductSerialDocument;
pub use inventory_model::StockBatchDocument;
pub use inventory_model::StockMovementDocument;
pub use inventory_model::SubcategoryDocument;
