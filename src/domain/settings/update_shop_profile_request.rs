use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Request payload to update the shop profile.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateShopProfileRequest {
    #[serde(default)]
    pub legal_name: Option<String>,
    #[serde(default)]
    pub trading_name: Option<String>,
    #[serde(default)]
    pub address_lines: Option<Vec<String>>,
    #[serde(default)]
    pub primary_phone: Option<String>,
    #[serde(default)]
    pub secondary_phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub business_reg_no: Option<String>,
    #[serde(default)]
    pub logo_base64: Option<String>,
    #[serde(default)]
    pub bank_name: Option<String>,
    #[serde(default)]
    pub bank_branch: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub account_number: Option<String>,
    #[serde(default)]
    pub default_warranty_text: Option<String>,
    #[serde(default)]
    pub default_footer_text: Option<String>,
    #[serde(default)]
    pub receipt_footer_text: Option<String>,
    #[serde(default, rename = "allowSharedUserSessions")]
    pub allow_shared_user_sessions: Option<bool>,
}
