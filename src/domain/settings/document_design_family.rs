use super::design_option::DesignOption;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// One design in a document kind's dropdown: the latest published version of
/// a design family.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDesignFamily {
    /// The design family's stable id — what a shop stores. For a bundled
    /// design it is the template's own `doc_temp_...` name.
    pub family: String,
    /// The label to show (the template's schema title).
    pub title: String,
    /// Latest published version of the family.
    pub version: i64,
    /// True for the kind's built-in "standard design".
    pub is_default: bool,
    /// Controls the design offers; empty when it has none.
    pub options: Vec<DesignOption>,
}
