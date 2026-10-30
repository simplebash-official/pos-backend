use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether a product's labels are up to date. `Never`: in stock and never
/// labelled. `Needed`: labelled before, but units entered since are still on
/// the shelf. `Labelled`: everything else that has been labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelCoverageStatus {
    Never,
    Needed,
    Labelled,
}
