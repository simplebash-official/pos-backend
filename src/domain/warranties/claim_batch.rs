//! Opening several claims on one invoice in one request: what the counter sends and the
//! per-item answer it gets back. Each item becomes its own ordinary claim; nothing here links
//! them afterwards.

use super::claim::{VoidChecks, WarrantyClaim};
use crate::domain::users::ManagerApproval;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `POST /warranty-claims/batch`. Every warranty must belong to the same invoice.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenClaimsBatchRequest {
    /// One entry per warranty to claim; at most 100, none twice.
    pub items: Vec<BatchClaimItem>,
    /// Needed only when at least one warranty has already run out and the caller is Staff: one
    /// manager or owner PIN and reason, checked once and used for the warranties that ran out.
    /// Sending one when none has run out is a 400.
    pub approval: Option<ManagerApproval>,
}

/// One claim of a batch. The fields mean what they do on `OpenClaimRequest`; the issue is
/// written out for every item, the server never copies one item's text to another.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchClaimItem {
    pub warranty_key: String,
    pub issue_description: String,
    pub void_checks: Option<VoidChecks>,
    pub conditions: Option<Vec<String>>,
    /// How many units of this warranty are being claimed.
    pub quantity: Option<i64>,
    /// The specific serial number being claimed, if the warranty does not name one.
    pub serial_number: Option<String>,
}

/// Why one item of a batch was not opened: the status, code and message its own request would
/// have answered with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchClaimError {
    pub status: u16,
    pub code: String,
    pub message: String,
}

/// What happened to one item, in the order the items were sent.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum BatchClaimResult {
    /// The claim was opened.
    Opened {
        #[serde(rename = "warrantyKey")]
        warranty_key: String,
        claim: Box<WarrantyClaim>,
    },
    /// Nothing was written for this item.
    Failed {
        #[serde(rename = "warrantyKey")]
        warranty_key: String,
        error: BatchClaimError,
    },
}

/// The answer to a batch: one result per item, so a failed item never hides an opened one.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenClaimsBatchResponse {
    pub results: Vec<BatchClaimResult>,
    pub opened_count: u32,
    pub failed_count: u32,
}
