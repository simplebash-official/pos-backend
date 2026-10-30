use super::document_design_family::DocumentDesignFamily;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// A kind of printed document and the designs a shop can pick for it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentKindSettings {
    /// `a4-invoice`, `thermal-receipt`, `credit-note` or `quotation`.
    pub kind: String,
    pub label: String,
    /// The family the shop chose, or `null` for the standard design.
    pub selected: Option<String>,
    /// The family prints use right now. `null` when `selected` is no longer
    /// offered: printing this kind fails (422 `TEMPLATE_UNAVAILABLE`) until a
    /// design is chosen.
    pub effective: Option<String>,
    /// The standard design first, then the rest by title.
    pub families: Vec<DocumentDesignFamily>,
    /// The option values in force per family offered here that declares
    /// options (`family -> key -> value`): what the shop saved, else the
    /// design's declared defaults.
    pub option_values: HashMap<String, HashMap<String, serde_json::Value>>,
}
