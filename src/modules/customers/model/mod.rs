//! Mongo document shape for the customers feature. Kept separate from
//! `domain::customers` (the API-facing types) so BSON concerns like
//! `ObjectId` never leak into request/response payloads.

mod customer_document;

pub use customer_document::CustomerDocument;
