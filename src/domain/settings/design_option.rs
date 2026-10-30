use super::design_option_choice::DesignOptionChoice;
use super::design_option_type::DesignOptionType;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesignOption {
    /// Key inside the print payload's `options` object.
    pub key: String,
    pub r#type: DesignOptionType,
    pub label: String,
    /// A bool for `toggle`, a choice value for `select`, `#RRGGBB` for
    /// `color`, text for `text`. Declared by the design itself.
    pub default: serde_json::Value,
    /// Present (and non-empty) for `select` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<DesignOptionChoice>>,
    /// Present for `text` only (1-200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
}
