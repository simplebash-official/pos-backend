use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /customers/tags` — every distinct tag currently in use
/// across all customers' `tags` arrays, for client-side autocompletion.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerTagsResponse {
    /// List of distinct customer tags.
    pub tags: Vec<String>,
}
