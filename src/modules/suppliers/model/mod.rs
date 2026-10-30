//! Mongo document shape for the suppliers feature. Kept separate from
//! `domain::suppliers` (the API-facing types) so BSON concerns like
//! `ObjectId` never leak into request/response payloads.

mod supplier_document;

pub use supplier_document::SupplierDocument;
