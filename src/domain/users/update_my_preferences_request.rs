use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /auth/me/preferences`. Only fields present are changed.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMyPreferencesRequest {
    /// New billing catalog sort id; must be one of `CATALOG_SORT_IDS`.
    pub billing_catalog_sort: Option<String>,
}
