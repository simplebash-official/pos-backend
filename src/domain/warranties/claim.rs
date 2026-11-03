//! A warranty claim (RMA): a customer brings a covered item back with a fault. It moves through
//! a fixed set of steps, a manager decides the outcome, and every step is logged on the
//! warranty's activity.

use crate::domain::users::ManagerApproval;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Where a claim is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// Taken in from the customer.
    Received,
    /// Looked at; notes written.
    Inspected,
    /// A manager or owner chose repair, replace or refund.
    Decided,
    /// Sent to the supplier or maker.
    AtSupplier,
    /// The outcome is done (repaired, replaced, refunded or rejected).
    Resolved,
    /// Handed back to the customer.
    Returned,
}

impl ClaimStatus {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimStatus::Received => "received",
            ClaimStatus::Inspected => "inspected",
            ClaimStatus::Decided => "decided",
            ClaimStatus::AtSupplier => "at_supplier",
            ClaimStatus::Resolved => "resolved",
            ClaimStatus::Returned => "returned",
        }
    }

    /// Still being worked on (not yet resolved).
    #[must_use]
    pub fn is_open(self) -> bool {
        !matches!(self, ClaimStatus::Resolved | ClaimStatus::Returned)
    }
}

/// What a manager decided to do about a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimDecision {
    /// Fix the item and give it back; the warranty carries on.
    Repair,
    /// Give a new unit; the new unit's warranty runs to the old end date.
    Replace,
    /// Give the money back through a credit note; the warranty ends.
    Refund,
    /// Not covered (damage, tampering, expired); the item goes back as it came.
    Reject,
}

impl ClaimDecision {
    /// The name this value has on the wire and in storage.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimDecision::Repair => "repair",
            ClaimDecision::Replace => "replace",
            ClaimDecision::Refund => "refund",
            ClaimDecision::Reject => "reject",
        }
    }
}

/// A step taken on a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimAction {
    Inspect,
    Decide(ClaimDecision),
    SendToSupplier,
    SupplierReturn,
    Resolve,
    /// A refund completed by its credit note.
    Refunded,
    ReturnToCustomer,
}

/// Why a step is not allowed now, worded for the person at the counter.
fn refused(message: &'static str) -> Result<ClaimStatus, &'static str> {
    Err(message)
}

/// The status a claim moves to when `action` is taken, or why it cannot be taken. The one place
/// the claim lifecycle is written down: received → inspected → decided → (at supplier) →
/// resolved → returned. A rejection resolves at once; a refund is resolved by its credit note
/// and is never handed back.
///
/// # Errors
///
/// A sentence saying why the step is not allowed from where the claim is.
pub fn next_status(
    current: ClaimStatus,
    decision: Option<ClaimDecision>,
    action: ClaimAction,
) -> Result<ClaimStatus, &'static str> {
    use ClaimAction as A;
    use ClaimStatus as S;
    match (action, current) {
        (A::Inspect, S::Received) => Ok(S::Inspected),
        (A::Inspect, _) => refused("This claim has already been inspected."),
        (A::Decide(_), S::Inspected) => match action {
            A::Decide(ClaimDecision::Reject) => Ok(S::Resolved),
            _ => Ok(S::Decided),
        },
        (A::Decide(_), S::Received) => refused("Inspect the item before deciding."),
        (A::Decide(_), _) => refused("This claim has already been decided."),
        (A::SendToSupplier, S::Inspected | S::Decided) => match decision {
            Some(ClaimDecision::Refund | ClaimDecision::Reject) => {
                refused("A refunded or rejected claim is not sent to the supplier.")
            }
            _ => Ok(S::AtSupplier),
        },
        (A::SendToSupplier, S::AtSupplier) => refused("This item is already with the supplier."),
        (A::SendToSupplier, _) => refused("Inspect the item before sending it to the supplier."),
        (A::SupplierReturn, S::AtSupplier) => Ok(if decision.is_some() {
            S::Decided
        } else {
            S::Inspected
        }),
        (A::SupplierReturn, _) => refused("This item is not with the supplier."),
        (A::Resolve, S::Decided) => match decision {
            Some(ClaimDecision::Repair | ClaimDecision::Replace) => Ok(S::Resolved),
            Some(ClaimDecision::Refund) => {
                refused("Give the refund with a credit note from this claim.")
            }
            _ => refused("Decide what to do before finishing the claim."),
        },
        (A::Resolve, S::AtSupplier) => {
            refused("Record that the item is back from the supplier first.")
        }
        (A::Resolve, _) => refused("Decide what to do before finishing the claim."),
        (A::Refunded, S::Decided) if decision == Some(ClaimDecision::Refund) => Ok(S::Resolved),
        (A::Refunded, _) => refused("Only a claim decided as a refund is refunded."),
        (A::ReturnToCustomer, S::Resolved) => match decision {
            Some(ClaimDecision::Refund) => refused("A refunded item stays with the shop."),
            _ => Ok(S::Returned),
        },
        (A::ReturnToCustomer, S::Returned) => refused("This item was already given back."),
        (A::ReturnToCustomer, _) => refused("Finish the claim before giving the item back."),
    }
}

/// What staff saw on the item when it came in; any of these may void the warranty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidChecks {
    pub screen_cracked: bool,
    pub liquid_damage: bool,
    pub seal_broken: bool,
    pub tampered: bool,
}

/// What the supplier did with an item sent to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierOutcome {
    Repaired,
    Replaced,
    Credited,
    Rejected,
}

/// The part of a claim sent on to a supplier or maker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierClaim {
    pub supplier_key: String,
    /// The supplier's name when it was sent.
    pub supplier_name: String,
    /// `YYYY-MM-DD`.
    pub sent_on: String,
    /// The supplier's own reference for the claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// `YYYY-MM-DD`, when the supplier said it would come back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_back_on: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_back_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<SupplierOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One claim as the API returns it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyClaim {
    pub id: String,
    /// `wcl_…`
    pub key: String,
    /// `WCL-000001`
    pub claim_number: String,
    pub warranty_key: String,
    pub warranty_number: String,
    pub item_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub invoice_key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// What the customer says is wrong.
    pub issue_description: String,
    pub void_checks: VoidChecks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    pub status: ClaimStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ClaimDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by_name: Option<String>,
    /// Who allowed a claim on a warranty that had already run out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_override_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_warranty_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    /// The paid repair job made from a rejected claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_repair_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid_repair_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierClaim>,
    /// `YYYY-MM-DD`, shop-local.
    pub received_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_on: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_on: Option<String>,
    /// Read-time: days since it came in.
    pub age_days: i64,
    /// Read-time: with the supplier past the day they said it would be back.
    pub supplier_overdue: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// `POST /warranty-claims`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenClaimRequest {
    pub warranty_key: String,
    pub issue_description: String,
    pub void_checks: VoidChecks,
    /// The technician who will look at it.
    pub assigned_employee_key: Option<String>,
    /// Needed only when the warranty has already run out: a manager or owner honours it.
    pub approval: Option<ManagerApproval>,
}

/// `POST /warranty-claims/{key}/inspect`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectClaimRequest {
    pub inspection_notes: String,
    /// Changes the technician; absent keeps who it is.
    pub assigned_employee_key: Option<String>,
}

/// `POST /warranty-claims/{key}/decide`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecideClaimRequest {
    pub decision: ClaimDecision,
    /// The deciding manager's or owner's PIN and why.
    pub approval: ManagerApproval,
}

/// `POST /warranty-claims/{key}/supplier/send`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendToSupplierRequest {
    pub supplier_key: String,
    pub reference: Option<String>,
    /// `YYYY-MM-DD`.
    pub expected_back_on: Option<String>,
}

/// `POST /warranty-claims/{key}/supplier/return`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierReturnRequest {
    pub outcome: SupplierOutcome,
    pub note: Option<String>,
}

/// The replacement unit given on a replace.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementRequest {
    /// For a serialized item: the in-stock unit given instead.
    pub serial_number: Option<String>,
    /// For a non-serialized item: the price tier the new unit is taken from.
    pub selling_price_cents: Option<i64>,
}

/// `POST /warranty-claims/{key}/resolve`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveClaimRequest {
    pub resolution_note: Option<String>,
    /// Required for a replace, refused otherwise.
    pub replacement: Option<ReplacementRequest>,
}

/// Which claims a list wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimListStatus {
    /// Not yet resolved.
    Open,
    /// Every claim.
    All,
    Received,
    Inspected,
    Decided,
    AtSupplier,
    Resolved,
    Returned,
}

/// Query for `GET /warranty-claims`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ClaimListQuery {
    /// Which claims (`open`, `all`, or one status).
    pub status: ClaimListStatus,
    pub warranty_key: Option<String>,
    pub invoice_key: Option<String>,
    pub supplier_key: Option<String>,
    pub assigned_employee_key: Option<String>,
    /// Matches the claim number, warranty number, serial, item, customer or phone.
    pub q: Option<String>,
    pub page: Option<u64>,
    /// At most 200.
    pub limit: Option<u64>,
}

/// `GET /warranty-claims`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaimListResponse {
    /// Oldest first, so what has waited longest is on top.
    pub items: Vec<WarrantyClaim>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClaimAction as A;
    use ClaimDecision as D;
    use ClaimStatus as S;

    #[test]
    fn the_happy_paths_follow_the_lifecycle() {
        assert_eq!(next_status(S::Received, None, A::Inspect), Ok(S::Inspected));
        assert_eq!(
            next_status(S::Inspected, None, A::Decide(D::Repair)),
            Ok(S::Decided)
        );
        assert_eq!(
            next_status(S::Decided, Some(D::Repair), A::Resolve),
            Ok(S::Resolved)
        );
        assert_eq!(
            next_status(S::Resolved, Some(D::Repair), A::ReturnToCustomer),
            Ok(S::Returned)
        );
        assert_eq!(
            next_status(S::Decided, Some(D::Replace), A::Resolve),
            Ok(S::Resolved)
        );
        assert_eq!(
            next_status(S::Inspected, None, A::Decide(D::Reject)),
            Ok(S::Resolved)
        );
        assert_eq!(
            next_status(S::Resolved, Some(D::Reject), A::ReturnToCustomer),
            Ok(S::Returned)
        );
        assert_eq!(
            next_status(S::Decided, Some(D::Refund), A::Refunded),
            Ok(S::Resolved)
        );
    }

    #[test]
    fn a_supplier_trip_comes_back_to_where_it_left() {
        assert_eq!(
            next_status(S::Inspected, None, A::SendToSupplier),
            Ok(S::AtSupplier)
        );
        assert_eq!(
            next_status(S::AtSupplier, None, A::SupplierReturn),
            Ok(S::Inspected)
        );
        assert_eq!(
            next_status(S::Decided, Some(D::Replace), A::SendToSupplier),
            Ok(S::AtSupplier)
        );
        assert_eq!(
            next_status(S::AtSupplier, Some(D::Replace), A::SupplierReturn),
            Ok(S::Decided)
        );
        assert!(next_status(S::Decided, Some(D::Refund), A::SendToSupplier).is_err());
        assert!(next_status(S::AtSupplier, Some(D::Repair), A::Resolve).is_err());
    }

    #[test]
    fn steps_out_of_order_are_refused() {
        let every_status = [
            S::Received,
            S::Inspected,
            S::Decided,
            S::AtSupplier,
            S::Resolved,
            S::Returned,
        ];
        for status in every_status {
            if status != S::Received {
                assert!(next_status(status, None, A::Inspect).is_err(), "{status:?}");
            }
            if status != S::Inspected {
                assert!(
                    next_status(status, None, A::Decide(D::Repair)).is_err(),
                    "{status:?}"
                );
            }
            if status != S::Resolved {
                assert!(
                    next_status(status, Some(D::Repair), A::ReturnToCustomer).is_err(),
                    "{status:?}"
                );
            }
        }
        assert!(next_status(S::Decided, Some(D::Refund), A::Resolve).is_err());
        assert!(next_status(S::Resolved, Some(D::Refund), A::ReturnToCustomer).is_err());
        assert!(next_status(S::Decided, Some(D::Repair), A::Refunded).is_err());
        assert!(next_status(S::Inspected, None, A::Resolve).is_err());
    }

    #[test]
    fn only_unresolved_claims_are_open() {
        assert!(S::Received.is_open());
        assert!(S::AtSupplier.is_open());
        assert!(!S::Resolved.is_open());
        assert!(!S::Returned.is_open());
    }
}
