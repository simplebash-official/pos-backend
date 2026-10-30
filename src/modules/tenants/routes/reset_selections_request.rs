use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetSelectionsRequest {
    /// Design families whose choice (and saved option values) is cleared.
    pub families: Vec<String>,
}
