use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Which design printed a stored document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredDocumentDesign {
    /// The template name it was rendered with; `null` for a document stored
    /// before designs were recorded.
    pub template_ref: Option<String>,
    pub title: Option<String>,
    pub version: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
