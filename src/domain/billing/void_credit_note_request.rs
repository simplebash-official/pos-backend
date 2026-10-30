use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/credit-notes/{id}/void`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidCreditNoteRequest {
    /// Mandatory reason explaining why the credit note is being voided.
    pub reason: String,
}
