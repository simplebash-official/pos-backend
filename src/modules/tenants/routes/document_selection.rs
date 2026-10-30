use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSelection {
    /// The document kind (`a4-invoice`, `thermal-receipt`, `credit-note`, `quotation`).
    pub kind: String,
    /// The design family the shop chose for it.
    pub family: String,
}
