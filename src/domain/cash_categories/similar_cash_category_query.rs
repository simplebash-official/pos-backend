use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /cash-categories/similar`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SimilarCashCategoryQuery {
    pub movement_type: String,
    pub name: String,
}
