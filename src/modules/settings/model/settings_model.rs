use crate::domain::settings::ShopProfileResponse;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The shop's chosen design family per document kind (`kind slug -> family`)
/// and the option values it saved per design (`family -> key -> value`).
/// One row per shop (key `main`); tenant-scoped like `shop_profiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTemplateSettingsDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub selections: HashMap<String, String>,
    pub options: HashMap<String, HashMap<String, serde_json::Value>>,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// How a shop profile is stored in the database.
pub struct ShopProfileDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
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
    pub allow_shared_user_sessions: bool,
    pub version: i64,
    pub created_at: BsonDateTime,
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl ShopProfileDocument {
    #[must_use]
    /// Converts the stored document into the type the API returns.
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
            allow_shared_user_sessions: self.allow_shared_user_sessions,
        }
    }
}
