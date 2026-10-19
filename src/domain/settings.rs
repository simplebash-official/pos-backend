// Pure business types for settings and shop profile persistence.

use std::collections::HashMap;

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

/// Control a design declares for the shop to adjust (`x-options` in its
/// schema), e.g. a signature-box toggle or an accent colour.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesignOptionType {
    Toggle,
    Select,
    Color,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesignOptionChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesignOption {
    /// Key inside the print payload's `options` object.
    pub key: String,
    pub r#type: DesignOptionType,
    pub label: String,
    /// A bool for `toggle`, a choice value for `select`, `#RRGGBB` for
    /// `color`, text for `text`. Declared by the design itself.
    pub default: serde_json::Value,
    /// Present (and non-empty) for `select` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<DesignOptionChoice>>,
    /// Present for `text` only (1-200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
}

/// One design in a document kind's dropdown: the latest published version of
/// a design family.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDesignFamily {
    /// The design family's stable id — what a shop stores. For a bundled
    /// design it is the template's own `doc_temp_...` name.
    pub family: String,
    /// The label to show (the template's schema title).
    pub title: String,
    /// Latest published version of the family.
    pub version: i64,
    /// True for the kind's built-in "standard design".
    pub is_default: bool,
    /// Controls the design offers; empty when it has none.
    pub options: Vec<DesignOption>,
}

/// A kind of printed document and the designs a shop can pick for it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentKindSettings {
    /// `a4-invoice`, `thermal-receipt`, `credit-note` or `quotation`.
    pub kind: String,
    pub label: String,
    /// The family the shop chose, or `null` for the standard design.
    pub selected: Option<String>,
    /// The family prints use right now. `null` when `selected` is no longer
    /// offered: printing this kind fails (422 `TEMPLATE_UNAVAILABLE`) until a
    /// design is chosen.
    pub effective: Option<String>,
    /// The standard design first, then the rest by title.
    pub families: Vec<DocumentDesignFamily>,
    /// The option values in force per family offered here that declares
    /// options (`family -> key -> value`): what the shop saved, else the
    /// design's declared defaults.
    pub option_values: HashMap<String, HashMap<String, serde_json::Value>>,
}

/// The shop's document designs, one entry per kind.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTemplatesResponse {
    pub kinds: Vec<DocumentKindSettings>,
}

/// Request payload to change the shop's designs. Only what is present
/// changes: `selections` maps a kind to a family (`null` returns the kind to
/// the standard design); `options` replaces the given values of a family.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentTemplatesRequest {
    pub selections: HashMap<String, Option<String>>,
    pub options: HashMap<String, HashMap<String, serde_json::Value>>,
}

/// Which design printed a stored document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredDocumentDesign {
    /// The template name it was rendered with; `null` for a document stored
    /// before designs were recorded.
    pub template_ref: Option<String>,
    pub title: Option<String>,
    pub version: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CurrentDocumentDesign {
    pub template_ref: String,
    pub title: String,
    pub version: i64,
}

/// Answer of the design status endpoints: what a stored document was printed
/// with versus what would print now.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDesignStatus {
    pub stored: Option<StoredDocumentDesign>,
    pub current: CurrentDocumentDesign,
    /// True when a stored document exists and was printed with another design.
    pub differs: bool,
}

/// `design` query parameter of the document endpoints.
#[derive(Debug, Clone, Copy, serde::Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DocumentDesignChoice {
    /// The stored PDF, exactly as first printed.
    Original,
    /// The design in force now (rendered if not yet stored for it).
    Current,
}
