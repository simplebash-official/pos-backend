use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentDocumentDesign {
    pub template_ref: String,
    pub title: String,
    pub version: i64,
}
