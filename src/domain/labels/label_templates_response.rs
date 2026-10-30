use super::label_template::LabelTemplate;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `GET /labels/templates` result: starters first, then the shop's own by name.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplatesResponse {
    pub templates: Vec<LabelTemplate>,
}
