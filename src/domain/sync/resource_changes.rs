use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Change delta container for a specific entity type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceChanges<T: Serialize> {
    /// Indicates if a full snapshot is returned (e.g. initial sync).
    pub full: bool,
    /// List of created or updated records.
    pub items: Vec<T>,
    /// List of keys for records deleted since the cursor.
    pub deleted: Vec<String>,
    /// Next cursor token to pass on subsequent sync calls.
    pub next_cursor: Option<String>,
    /// Whether more changes remain to be fetched.
    pub has_more: bool,
    /// True when the request carried a cursor and nothing has changed since.
    /// Distinguishes "no new rows" from "same cursor echoed back", which the
    /// client cannot otherwise tell apart.
    pub unchanged: bool,
}
