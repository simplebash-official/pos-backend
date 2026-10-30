use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /auth/sessions`. `user_id` filters to one
/// account's login history; omitted returns every account's history
/// (subject to `SESSIONS_VIEW` gating). Paginated (unlike `suppliers`'
/// unpaginated lists) since this is an append-only log that grows without
/// bound, unlike a shop's small, roughly-static staff/supplier roster.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LoginSessionListQuery {
    /// Optional user ID or key to filter sessions for a specific user.
    pub user_id: Option<String>,
    /// Page number for pagination (defaults to 1).
    pub page: Option<u64>,
    /// Number of session records to return per page.
    pub limit: Option<u64>,
}
