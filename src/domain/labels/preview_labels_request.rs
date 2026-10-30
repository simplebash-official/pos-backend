use super::label_sample::LabelSample;
use super::label_template_input::LabelTemplateInput;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of `POST /labels/preview`: an unsaved design plus the sample to draw.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLabelsRequest {
    pub template: LabelTemplateInput,
    pub sample: LabelSample,
}
