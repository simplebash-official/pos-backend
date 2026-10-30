use super::label_element::LabelElement;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of `POST /labels/templates` and `PUT /labels/templates/{key}`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplateInput {
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
}
