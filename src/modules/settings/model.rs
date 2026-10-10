// Storage document shapes for settings.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::settings::ShopProfileResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopProfileDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    #[serde(default)]
    pub legal_name: String,
    #[serde(default)]
    pub trading_name: String,
    #[serde(default)]
    pub address_lines: Vec<String>,
    #[serde(default)]
    pub primary_phone: String,
    #[serde(default)]
    pub secondary_phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub business_reg_no: String,
    #[serde(default)]
    pub logo_base64: String,
    #[serde(default)]
    pub bank_name: String,
    #[serde(default)]
    pub bank_branch: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub account_number: String,
    #[serde(default)]
    pub default_warranty_text: String,
    #[serde(default)]
    pub default_footer_text: String,
    #[serde(default)]
    pub receipt_footer_text: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

fn default_version() -> i64 {
    1
}

impl ShopProfileDocument {
    pub fn into_response(self) -> ShopProfileResponse {
        ShopProfileResponse {
            id: self
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| self.key.clone()),
            version: self.version,
            legal_name: self.legal_name,
            trading_name: self.trading_name,
            address_lines: self.address_lines,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            email: self.email,
            website: self.website,
            business_reg_no: self.business_reg_no,
            logo_base64: self.logo_base64,
            bank_name: self.bank_name,
            bank_branch: self.bank_branch,
            account_name: self.account_name,
            account_number: self.account_number,
            default_warranty_text: self.default_warranty_text,
            default_footer_text: self.default_footer_text,
            receipt_footer_text: self.receipt_footer_text,
        }
    }
}
