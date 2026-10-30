// Mongo document shape for user accounts. Kept separate from
// `domain::users` (the API-facing types) so `password_hash` and BSON
// concerns like `ObjectId` never leak into request/response payloads —
// `into_user()` deliberately has no way to carry the hash forward.

mod user_document;

pub use user_document::UserDocument;
