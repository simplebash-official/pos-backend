//! Mongo document shape for quotations. Kept separate from
//! `domain::quotations` (the API-facing types) so BSON concerns like
//! `ObjectId` never leak into request/response payloads. Synced between devices
//! and the cloud (see `modules::sync::resources`), hence `version`, `deleted_at`
//! and `updated_by_device`.

mod quotation_document;

pub use quotation_document::QuotationDocument;
