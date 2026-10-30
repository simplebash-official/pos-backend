use super::document_selection::DocumentSelection;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetSelectionsResponse {
    /// The `(kind, family)` choices that were cleared.
    pub reset: Vec<DocumentSelection>,
}
