use super::serial_status::SerialStatus;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductSerialListQuery {
    /// Filter by lifecycle status (e.g. `in_stock` for a sale-time picker).
    pub status: Option<SerialStatus>,
}
