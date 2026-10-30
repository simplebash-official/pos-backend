// Mongo document shape for the repairs feature. Kept separate from
// `domain::repairs` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads.

mod repair_document;

pub use repair_document::RepairDocument;
