use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Control a design declares for the shop to adjust (`x-options` in its
/// schema), e.g. a signature-box toggle or an accent colour.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesignOptionType {
    Toggle,
    Select,
    Color,
    Text,
}
