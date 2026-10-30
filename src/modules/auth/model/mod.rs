// Mongo document shape for the `login_sessions` audit log — one row per
// successful login. Kept separate from `domain::auth` (the API-facing
// types) so BSON concerns like `ObjectId` never leak into responses.

mod login_session_document;

pub use login_session_document::LoginSessionDocument;
