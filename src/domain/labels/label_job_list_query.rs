use super::label_print_action::LabelPrintAction;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query of `GET /labels/jobs`. `from`/`to` are RFC 3339 instants (`to` exclusive).
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LabelJobListQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Only jobs that printed this product.
    pub product_key: Option<String>,
    pub action: Option<LabelPrintAction>,
}
