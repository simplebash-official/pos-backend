/// `design` query parameter of the document endpoints.
#[derive(Debug, Clone, Copy, serde::Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DocumentDesignChoice {
    /// The stored PDF, exactly as first printed.
    Original,
    /// The design in force now (rendered if not yet stored for it).
    Current,
}
