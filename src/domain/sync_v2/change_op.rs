use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Whether a change writes the row or tombstones it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ChangeOp {
    Upsert,
    Delete,
}
