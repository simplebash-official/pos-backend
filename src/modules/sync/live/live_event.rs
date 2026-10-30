use mongodb::bson::doc;
use serde::Serialize;

use utoipa::ToSchema;

/// One appended change-log row, as announced to subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    /// Position in the tenant's change log; pull with `since` below it.
    pub seq: i64,
    /// Wire resource name, e.g. `products`.
    pub resource: String,
    /// Who made the change: a device id, `web:<browser id>` or `cloud`.
    pub origin: Option<String>,
}
