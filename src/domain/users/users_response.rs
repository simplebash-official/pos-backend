use super::user::User;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /users`. Not paginated — a shop's staff roster is
/// small enough to return in full, matching `suppliers`' list contract.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsersResponse {
    /// Complete list of manageable users.
    pub users: Vec<User>,
}
