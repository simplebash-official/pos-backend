// Pure business types for settings and shop profile persistence.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The shop profile configuration returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShopProfileResponse {
    pub id: String,
    pub version: i64,
    pub legal_name: String,
    pub trading_name: String,
    pub address_lines: Vec<String>,
    pub primary_phone: String,
    pub secondary_phone: String,
    pub email: String,
    pub website: String,
    pub business_reg_no: String,
    pub logo_base64: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_name: String,
    pub account_number: String,
    pub default_warranty_text: String,
    pub default_footer_text: String,
    pub receipt_footer_text: String,
}

impl Default for ShopProfileResponse {
    fn default() -> Self {
        Self {
            id: "shop-main".to_string(),
            version: 1,
            legal_name: String::new(),
            trading_name: String::new(),
            address_lines: Vec::new(),
            primary_phone: String::new(),
            secondary_phone: String::new(),
            email: String::new(),
            website: String::new(),
            business_reg_no: String::new(),
            logo_base64: String::new(),
            bank_name: String::new(),
            bank_branch: String::new(),
            account_name: String::new(),
            account_number: String::new(),
            default_warranty_text: String::new(),
            default_footer_text: String::new(),
            receipt_footer_text: String::new(),
        }
    }
}

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
}
