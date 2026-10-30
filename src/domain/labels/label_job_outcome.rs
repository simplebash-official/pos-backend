use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a print job stands. `Rendered` until the computer reports back; a
/// download is `Sent` from the start (the file was handed over). A `Failed`
/// job does not count as labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelJobOutcome {
    Rendered,
    Sent,
    Failed,
}
