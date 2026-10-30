use serde::Deserialize;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// A shift's count against the drawer
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftCountRequest {
    /// Counting at close: the open shift.
    pub session_key: Option<String>,
    /// Counting at open: the terminal whose drawer is being counted.
    pub terminal_id: Option<String>,
    pub counted_cents: i64,
    /// At close, the float left in the drawer for the next shift. Absent until the person chooses
    /// one; the reply then suggests keeping the opening float.
    pub closing_float_cents: Option<i64>,
}
