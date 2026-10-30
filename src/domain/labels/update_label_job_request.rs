use super::label_job_outcome::LabelJobOutcome;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body of `PATCH /labels/jobs/{key}`: what the printer step did. `outcome`
/// must be `sent` or `failed`; `error` is the reason, only with `failed`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLabelJobRequest {
    pub outcome: LabelJobOutcome,
    pub error: Option<String>,
}
