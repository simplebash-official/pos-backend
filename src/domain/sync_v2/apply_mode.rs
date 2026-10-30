use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ApplyMode {
    Incremental,
    Bootstrap,
}
