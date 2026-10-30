use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Horizontal alignment of a text element inside its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}
