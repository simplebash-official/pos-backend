use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Per-login UI preferences, stored on the user document so each login in a
/// shop (admin, manager, every staff/employee account) keeps its own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserPreferences {
    /// Last sort method picked on the billing catalog (one of `CATALOG_SORT_IDS`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_catalog_sort: Option<String>,
}
