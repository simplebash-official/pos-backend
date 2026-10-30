use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing and filtering credit note records.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CreditNoteListQuery {
    /// Search term matching credit note number, invoice number, or customer name.
    pub search: Option<String>,
    /// Filter by original invoice key.
    pub invoice_key: Option<String>,
    /// Filter by customer key.
    pub customer_key: Option<String>,
    /// Scopes to credit notes created today when set to `"today"`.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page item limit.
    pub limit: Option<u64>,
}
