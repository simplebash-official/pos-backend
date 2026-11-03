//! One warranty: a covered unit (a sold serial, an invoice line, or later a repair item) with
//! its period, and the actions staff take on it.

use super::profile::WarrantyType;
use crate::domain::users::ManagerApproval;
use chrono::{DateTime, Utc};
use pos_calc_core::warranty::WarrantyCoverage;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// What the warranty came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantySourceType {
    /// An item sold on an invoice.
    ProductSale,
    /// Work done on a customer's device.
    Repair,
}

impl WarrantySourceType {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarrantySourceType::ProductSale => "product_sale",
            WarrantySourceType::Repair => "repair",
        }
    }
}

/// The stored status. Whether the period has run out is never stored: see `WarrantyState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyStatus {
    /// In force (it may still have run out by date).
    Active,
    /// A claim on it is open.
    UnderClaim,
    /// Cancelled: the sale was voided, the item came back, or a replacement took its place.
    Void,
}

impl WarrantyStatus {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarrantyStatus::Active => "active",
            WarrantyStatus::UnderClaim => "under_claim",
            WarrantyStatus::Void => "void",
        }
    }
}

impl std::str::FromStr for WarrantyStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(WarrantyStatus::Active),
            "under_claim" => Ok(WarrantyStatus::UnderClaim),
            "void" => Ok(WarrantyStatus::Void),
            other => Err(format!("unknown warranty status `{other}`")),
        }
    }
}

/// Where a warranty stands today, worked out on every read from its status and period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyState {
    Active,
    /// Active, in its last week.
    ExpiringSoon,
    /// Active by status, but the last covered day has passed.
    Expired,
    UnderClaim,
    Void,
}

/// One warranty as the API returns it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Warranty {
    pub id: String,
    /// `wty_…`
    pub key: String,
    /// `WTY-000001`, printed on the invoice and the warranty card.
    pub warranty_number: String,
    pub source_type: WarrantySourceType,
    pub invoice_key: String,
    pub invoice_number: String,
    /// Zero-based position of the covered line on its invoice.
    pub line_index: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_item_key: Option<String>,
    /// The item's name when the warranty began.
    pub item_name: String,
    /// The covered unit's serial or IMEI; absent for a non-serialized line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Units covered: 1 for a serial, the line's quantity otherwise.
    pub quantity: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// The customer's name when the warranty began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    /// The customer's phone when the warranty began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// The last nine digits of `customer_phone`, which a phone lookup matches on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_tail: Option<String>,
    pub coverage: WarrantyCoverage,
    pub warranty_type: WarrantyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    /// What one unit sold for.
    pub unit_price_cents: i64,
    /// First covered day, `YYYY-MM-DD`, shop-local.
    pub starts_on: String,
    /// Last covered day, `YYYY-MM-DD`; absent for a lifetime warranty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_on: Option<String>,
    pub status: WarrantyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub void_reason: Option<String>,
    /// The warranty this one replaced (a replacement unit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces_warranty_key: Option<String>,
    /// The warranty that replaced this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by_warranty_key: Option<String>,
    /// Claims ever opened on it.
    pub claim_count: i64,
    /// Read-time: where it stands today.
    pub state: WarrantyState,
    /// Read-time: covered days left today, the last day counting as one; absent for lifetime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_remaining: Option<i64>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// What happened to a warranty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyEventKind {
    Created,
    Voided,
    Extended,
    ClaimOpened,
    ClaimAdvanced,
    ClaimDecided,
    Replaced,
    ExpiredOverride,
    SupplierSent,
    SupplierReturned,
    ReturnedToCustomer,
}

impl WarrantyEventKind {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WarrantyEventKind::Created => "created",
            WarrantyEventKind::Voided => "voided",
            WarrantyEventKind::Extended => "extended",
            WarrantyEventKind::ClaimOpened => "claim_opened",
            WarrantyEventKind::ClaimAdvanced => "claim_advanced",
            WarrantyEventKind::ClaimDecided => "claim_decided",
            WarrantyEventKind::Replaced => "replaced",
            WarrantyEventKind::ExpiredOverride => "expired_override",
            WarrantyEventKind::SupplierSent => "supplier_sent",
            WarrantyEventKind::SupplierReturned => "supplier_returned",
            WarrantyEventKind::ReturnedToCustomer => "returned_to_customer",
        }
    }
}

impl std::str::FromStr for WarrantyEventKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "created" => WarrantyEventKind::Created,
            "voided" => WarrantyEventKind::Voided,
            "extended" => WarrantyEventKind::Extended,
            "claim_opened" => WarrantyEventKind::ClaimOpened,
            "claim_advanced" => WarrantyEventKind::ClaimAdvanced,
            "claim_decided" => WarrantyEventKind::ClaimDecided,
            "replaced" => WarrantyEventKind::Replaced,
            "expired_override" => WarrantyEventKind::ExpiredOverride,
            "supplier_sent" => WarrantyEventKind::SupplierSent,
            "supplier_returned" => WarrantyEventKind::SupplierReturned,
            "returned_to_customer" => WarrantyEventKind::ReturnedToCustomer,
            other => return Err(format!("unknown warranty event kind `{other}`")),
        })
    }
}

/// One line of a warranty's activity log: append-only, who did what and when.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyEvent {
    pub id: String,
    /// `wev_…`
    pub key: String,
    pub warranty_key: String,
    /// The claim the event belongs to, when it is a claim step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_key: Option<String>,
    pub kind: WarrantyEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Who did it (a user id, or `system` for automatic steps such as a voided sale).
    pub actor_user_id: String,
    pub actor_name: String,
    /// The manager or owner whose PIN approved it, when one was needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by_name: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// The invoice a warranty hangs off, as the detail screen shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyInvoiceLink {
    pub key: String,
    pub invoice_number: String,
    /// `pending`, `partially_paid`, `paid`, `voided` or `closed`.
    pub status: String,
    pub total_cents: i64,
    pub created_at: DateTime<Utc>,
}

/// `GET /warranties/{key}`: the warranty, its activity, and what it links to.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyDetail {
    pub warranty: Warranty,
    /// Oldest first.
    pub events: Vec<WarrantyEvent>,
    pub invoice: WarrantyInvoiceLink,
    /// The warranty this one replaced, then the one that replaced it, when they exist.
    pub replaces: Option<Warranty>,
    pub replaced_by: Option<Warranty>,
    /// Every claim on it, oldest first.
    pub claims: Vec<super::WarrantyClaim>,
}

/// Query for `GET /warranties/lookup`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WarrantyLookupQuery {
    /// A scanned or typed warranty number, serial / IMEI, invoice number or phone number.
    pub q: String,
}

/// What a lookup matched on, so the screen can say why these results came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarrantyLookupMatch {
    WarrantyNumber,
    Serial,
    InvoiceNumber,
    Phone,
    /// Nothing matched.
    Nothing,
}

/// The warranties of one device (one serial), or one non-serialized line.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// The newest warranty's item name.
    pub item_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// Newest first.
    pub warranties: Vec<Warranty>,
}

/// `GET /warranties/lookup`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyLookupResponse {
    pub matched_by: WarrantyLookupMatch,
    /// Groups ordered by their newest warranty, newest first.
    pub groups: Vec<WarrantyGroup>,
}

/// Query for `GET /warranties`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WarrantyListQuery {
    /// Only this customer's warranties.
    pub customer_key: Option<String>,
    /// Only the warranties of this invoice.
    pub invoice_key: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page (at most 200).
    pub limit: Option<u64>,
}

/// `GET /warranties`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyListResponse {
    /// Newest first.
    pub items: Vec<Warranty>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}

/// `POST /warranties/{key}/void`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidWarrantyRequest {
    /// A manager or owner PIN, and why the warranty is cancelled.
    pub approval: ManagerApproval,
}

/// `POST /warranties/{key}/extend`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtendWarrantyRequest {
    /// The new coverage, counted from the original first day; it must end later than now.
    pub coverage: WarrantyCoverage,
    /// A manager or owner PIN, and why it is extended.
    pub approval: ManagerApproval,
}
