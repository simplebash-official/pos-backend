use super::label_element::LabelElement;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A label design as the API returns it: a ready-made starter (`built_in`,
/// read-only, no timestamps) or one the shop built.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplate {
    /// `ltpl_<nanoid>` for a shop's template, `starter_<name>` for a built-in one.
    pub key: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
    pub built_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}
