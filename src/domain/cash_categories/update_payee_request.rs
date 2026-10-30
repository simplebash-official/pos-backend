use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /payees/{key}`. Absent fields are left unchanged.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePayeeRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// `Some(None)` is not expressible in JSON here, so clearing the phone sends an empty string.
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}
