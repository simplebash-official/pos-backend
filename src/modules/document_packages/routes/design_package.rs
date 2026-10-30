use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DesignPackage {
    /// The immutable template id (`doc_temp_...` / `lbl_temp_...`) of this version.
    pub name: String,
    pub family: String,
    pub version: i64,
    pub kind: String,
    pub title: String,
    /// `document` or `label`.
    pub r#type: String,
    /// SHA-256 (hex) of the archive.
    pub sha256: String,
    /// Oldest desktop app version that can use it, when it has a floor.
    pub min_app_version: Option<String>,
}
