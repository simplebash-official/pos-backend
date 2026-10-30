use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReturnedLine {
    /// Position of the line in the invoice's items.
    pub line_index: usize,
    /// Units being returned now (one for a serial-tracked unit).
    pub quantity: i64,
}
