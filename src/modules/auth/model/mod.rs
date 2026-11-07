//! Mongo document shape for the `login_sessions` audit log — one row per
//! successful login. Kept separate from `domain::auth` (the API-facing
//! types) so BSON concerns like `ObjectId` never leak into responses.

mod login_session_document;
mod password_reset_token_document;

pub use login_session_document::LoginSessionDocument;
pub use password_reset_token_document::PasswordResetTokenDocument;
