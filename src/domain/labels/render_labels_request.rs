use super::label_item::LabelItem;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of `POST /labels/render`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenderLabelsRequest {
    pub template_key: String,
    pub items: Vec<LabelItem>,
}
