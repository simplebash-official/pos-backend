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
    #[serde(rename = "allowSharedUserSessions")]
    pub allow_shared_user_sessions: bool,
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
            allow_shared_user_sessions: false,
        }
    }
}
