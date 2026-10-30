use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Request payload to change the shop's designs. Only what is present
/// changes: `selections` maps a kind to a family (`null` returns the kind to
/// the standard design); `options` replaces the given values of a family.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentTemplatesRequest {
    pub selections: HashMap<String, Option<String>>,
    pub options: HashMap<String, HashMap<String, serde_json::Value>>,
}
