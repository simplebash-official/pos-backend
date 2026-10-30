use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How a print left the computer: the system print window, or straight to a
/// named printer (desktop only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelPrintMethod {
    System,
    Direct,
}
