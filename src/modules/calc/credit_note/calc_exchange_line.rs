use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcExchangeLine {
    pub unit_price_cents: i64,
    pub quantity: i64,
}
