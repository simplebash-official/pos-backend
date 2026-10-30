use super::current_document_design::CurrentDocumentDesign;
use super::stored_document_design::StoredDocumentDesign;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Answer of the design status endpoints: what a stored document was printed
/// with versus what would print now.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDesignStatus {
    pub stored: Option<StoredDocumentDesign>,
    pub current: CurrentDocumentDesign,
    /// True when a stored document exists and was printed with another design.
    pub differs: bool,
}
