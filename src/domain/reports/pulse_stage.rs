use serde::Serialize;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Dashboard pulse
// ---------------------------------------------------------------------------

/// How many jobs sit at one stage of a pipeline, and their share of everything tracked.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseStage {
    /// `received`, `diagnosing`, `in_repair`, `ready` or `delivered`.
    pub stage: String,
    pub count: i64,
    /// Whole percent of every job tracked in the pipeline, rounded half up.
    pub percent: i64,
}
