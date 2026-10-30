use super::apply_mode::ApplyMode;
use super::pulled_change::PulledChange;

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub mode: ApplyMode,
    pub changes: Vec<PulledChange>,
    #[serde(default)]
    pub advance_cursor_to: Option<i64>,
    /// Bootstrap only, on the final page: the cloud shop has finished its own
    /// first-time setup. Absent or false = it has not, so this device must not
    /// declare its setup done on its own (the owner still chooses demo vs clean).
    #[serde(default)]
    pub setup_completed: Option<bool>,
    /// Bootstrap only: that cloud setup loaded the demo data.
    #[serde(default)]
    pub sample_data_loaded: Option<bool>,
}
