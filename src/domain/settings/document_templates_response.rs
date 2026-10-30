use super::document_kind_settings::DocumentKindSettings;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// The shop's document designs, one entry per kind.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTemplatesResponse {
    pub kinds: Vec<DocumentKindSettings>,
}
