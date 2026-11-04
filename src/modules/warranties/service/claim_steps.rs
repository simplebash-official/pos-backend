//! Opening a claim and every step that only moves it along: inspection, the manager's
//! decision, the trip to the supplier and handing the item back. What a decision does to stock,
//! money and the warranty itself is in `claim_outcomes`.

use super::warranty_events::{self, NewEvent, WarrantyActor};
use super::warranty_issue::next_number;
use super::warranty_lookup::require;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::utils::shop_day;
use crate::domain::warranties::{
    ClaimAction, ClaimDecision, ClaimListQuery, ClaimListResponse, ClaimListStatus, ClaimStatus,
    DecideClaimRequest, InspectClaimRequest, OpenClaimRequest, SendToSupplierRequest,
    SupplierClaim, SupplierReturnRequest, WarrantyClaim, WarrantyEventKind, WarrantyState,
    WarrantyStatus, next_status,
};
use crate::modules::warranties::model::{WarrantyClaimDocument, WarrantyDocument};
use crate::modules::warranties::repository::{self, ClaimFilter};
use crate::modules::{employees, suppliers, users};
use chrono::{NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::Document;

const LIST_LIMIT_DEFAULT: u64 = 50;
const LIST_LIMIT_MAX: u64 = 200;
/// Most claims one warranty or invoice shows.
const LINKED_CLAIMS_MAX: u64 = 200;

pub(super) fn today() -> NaiveDate {
    shop_day(Utc::now())
}

fn claim_conflict() -> AppError {
    AppError::conflict(
        codes::VERSION_CONFLICT,
        "Someone else changed this claim just now. Open it again and retry.",
    )
}

fn warranty_conflict() -> AppError {
    AppError::conflict(
        codes::VERSION_CONFLICT,
        "Someone else changed this warranty just now. Open it again and retry.",
    )
}

/// Text that must be filled in, trimmed.
fn required_text(value: &str, what: &str) -> AppResult<String> {
    let text = value.trim();
    if text.is_empty() {
        return Err(AppError::validation(format!("Write {what}.")));
    }
    Ok(text.to_string())
}

/// Text that may be left out; blank counts as left out.
fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// The status the claim moves to for `action`, or 409 `CLAIM_STEP_NOT_ALLOWED` saying why not.
pub(super) fn step(claim: &WarrantyClaimDocument, action: ClaimAction) -> AppResult<ClaimStatus> {
    next_status(claim.status, claim.decision, action)
        .map_err(|why| AppError::conflict(codes::CLAIM_STEP_NOT_ALLOWED, why))
}

/// The stored claim with this key, or 404 `CLAIM_NOT_FOUND`.
pub(super) async fn require_claim(db: &Db, key: &str) -> AppResult<WarrantyClaimDocument> {
    repository::find_claim(db, key)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Claim not found", codes::CLAIM_NOT_FOUND))
}

/// Writes the changed claim if nobody changed it since `expected_version`, bumping its version.
pub(super) async fn save_claim(
    db: &Db,
    claim: &mut WarrantyClaimDocument,
    expected_version: i64,
) -> AppResult<()> {
    claim.version = expected_version + 1;
    claim.updated_at = BsonDateTime::now();
    if !repository::replace_claim(db, claim, expected_version).await? {
        return Err(claim_conflict());
    }
    Ok(())
}

/// Writes the changed warranty if nobody changed it since `expected_version`, bumping its
/// version.
pub(super) async fn save_warranty(
    db: &Db,
    warranty: &mut WarrantyDocument,
    expected_version: i64,
) -> AppResult<()> {
    warranty.version = expected_version + 1;
    warranty.updated_at = BsonDateTime::now();
    if !repository::replace_warranty(db, warranty, expected_version).await? {
        return Err(warranty_conflict());
    }
    Ok(())
}

/// Logs a claim step on its warranty's activity.
pub(super) async fn log_step(
    db: &Db,
    claim: &WarrantyClaimDocument,
    kind: WarrantyEventKind,
    from: Option<ClaimStatus>,
    note: Option<String>,
    actor: &WarrantyActor,
    approved_by: Option<&crate::domain::users::ApprovedBy>,
) -> AppResult<()> {
    warranty_events::record(
        db,
        NewEvent {
            warranty_key: &claim.warranty_key,
            claim_key: Some(&claim.key),
            kind,
            from_status: from.map(ClaimStatus::as_str),
            to_status: Some(claim.status.as_str()),
            note,
            actor,
            approved_by,
        },
    )
    .await
}

/// Puts a warranty that was held by a claim back in force (the claim ended without using it
/// up: repaired, or rejected).
pub(super) async fn release_warranty(
    db: &Db,
    claim: &WarrantyClaimDocument,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let mut warranty = require(db, &claim.warranty_key).await?;
    if warranty.status != WarrantyStatus::UnderClaim {
        return Ok(());
    }
    let expected = warranty.version;
    warranty.status = WarrantyStatus::Active;
    save_warranty(db, &mut warranty, expected).await?;
    warranty_events::record(
        db,
        NewEvent {
            warranty_key: &warranty.key,
            claim_key: Some(&claim.key),
            kind: WarrantyEventKind::ClaimAdvanced,
            from_status: Some(WarrantyStatus::UnderClaim.as_str()),
            to_status: Some(WarrantyStatus::Active.as_str()),
            note: Some(format!("Back in force after {}", claim.claim_number)),
            actor,
            approved_by: None,
        },
    )
    .await
}

async fn employee_name(db: &Db, key: &str) -> AppResult<String> {
    Ok(employees::service::get_employee_by_key(db, key).await?.name)
}

/// Parses a `YYYY-MM-DD` day sent by the client.
fn requested_day(text: &str, what: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("{what} must be a date (YYYY-MM-DD).")))
}

/// Opens a claim on a warranty that is in force. A warranty that ran out needs a manager's or
/// owner's approval to be honoured; one already under a claim, or cancelled, cannot take
/// another. The warranty is held (`under_claim`) until the claim is decided.
pub(crate) async fn open_claim(
    db: &Db,
    body: OpenClaimRequest,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_opened", async move {
        let issue = required_text(&body.issue_description, "what the customer says is wrong")?;
        let mut warranty = require(db, &body.warranty_key).await?;
        let today = today();
        let state = warranty.clone().into_warranty(today)?.state;
        let approver = match (state, &body.approval) {
            (WarrantyState::Void, _) => {
                return Err(AppError::conflict(
                    codes::WARRANTY_NOT_ACTIVE,
                    "This warranty is cancelled, so a claim cannot be opened on it.",
                ));
            }
            (WarrantyState::UnderClaim, _) => {
                return Err(AppError::conflict(
                    codes::CLAIM_ALREADY_OPEN,
                    "This item already has an open claim. Carry on with that one.",
                ));
            }
            (WarrantyState::Expired, None) => {
                return Err(AppError::conflict(
                    codes::WARRANTY_EXPIRED,
                    "This warranty has run out. A manager or the owner can still honour it.",
                ));
            }
            (WarrantyState::Expired, Some(approval)) => {
                Some(users::service::require_manager_approval(db, approval).await?)
            }
            (WarrantyState::Active | WarrantyState::ExpiringSoon, Some(_)) => {
                return Err(AppError::validation(
                    "This warranty is still in force, so no approval is needed.",
                ));
            }
            (WarrantyState::Active | WarrantyState::ExpiringSoon, None) => None,
        };
        let assigned_employee_name = match &body.assigned_employee_key {
            Some(key) => Some(employee_name(db, key).await?),
            None => None,
        };

        // Reserved first, so a failed reservation never leaves the warranty held.
        let claim_number = next_number(db, "warranty_claim", device_id).await?;
        let expected = warranty.version;
        warranty.status = WarrantyStatus::UnderClaim;
        warranty.claim_count += 1;
        save_warranty(db, &mut warranty, expected).await?;

        let now = BsonDateTime::now();
        let claim = repository::insert_claim(
            db,
            WarrantyClaimDocument {
                id: None,
                key: generate_id(prefixes::WARRANTY_CLAIM),
                claim_number,
                warranty_key: warranty.key.clone(),
                warranty_number: warranty.warranty_number.clone(),
                item_name: warranty.item_name.clone(),
                serial_number: warranty.serial_number.clone(),
                product_key: warranty.product_key.clone(),
                invoice_key: warranty.invoice_key.clone(),
                invoice_number: warranty.invoice_number.clone(),
                customer_key: warranty.customer_key.clone(),
                customer_name: warranty.customer_name.clone(),
                customer_phone: warranty.customer_phone.clone(),
                issue_description: issue.clone(),
                void_checks: body.void_checks,
                assigned_employee_key: body.assigned_employee_key,
                assigned_employee_name,
                status: ClaimStatus::Received,
                inspection_notes: None,
                decision: None,
                decision_reason: None,
                decided_by_user_id: None,
                decided_by_name: None,
                expired_override_by_name: approver.as_ref().map(|a| a.name.clone()),
                resolution_note: None,
                replacement_serial_number: None,
                replacement_warranty_key: None,
                credit_note_key: None,
                paid_repair_key: None,
                paid_repair_number: None,
                rework_repair_key: None,
                rework_repair_number: None,
                supplier: None,
                received_on: today.to_string(),
                resolved_on: None,
                returned_on: None,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        if let Some(approver) = &approver {
            log_step(
                db,
                &claim,
                WarrantyEventKind::ExpiredOverride,
                None,
                Some(format!("Honoured after it ran out. {}", approver.reason)),
                actor,
                Some(approver),
            )
            .await?;
        }
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimOpened,
            None,
            Some(format!("{}: {issue}", claim.claim_number)),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today)
    })
    .await
}

/// Records what the technician found.
pub(crate) async fn inspect_claim(
    db: &Db,
    key: &str,
    body: InspectClaimRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_inspected", async move {
        let notes = required_text(&body.inspection_notes, "what the inspection found")?;
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::Inspect)?;
        if let Some(employee_key) = body.assigned_employee_key {
            claim.assigned_employee_name = Some(employee_name(db, &employee_key).await?);
            claim.assigned_employee_key = Some(employee_key);
        }
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.inspection_notes = Some(notes.clone());
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimAdvanced,
            Some(from),
            Some(notes),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}

/// A manager or the owner decides: repair, replace, refund or reject. A rejection ends the
/// claim at once and puts the warranty back in force.
pub(crate) async fn decide_claim(
    db: &Db,
    key: &str,
    body: DecideClaimRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_decided", async move {
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::Decide(body.decision))?;
        let approver = users::service::require_manager_approval(db, &body.approval).await?;
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.decision = Some(body.decision);
        claim.decision_reason = Some(approver.reason.clone());
        claim.decided_by_user_id = Some(approver.user_id.clone());
        claim.decided_by_name = Some(approver.name.clone());
        if to == ClaimStatus::Resolved {
            claim.resolved_on = Some(today().to_string());
        }
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimDecided,
            Some(from),
            Some(format!("{}: {}", body.decision.as_str(), approver.reason)),
            actor,
            Some(&approver),
        )
        .await?;
        if body.decision == ClaimDecision::Reject {
            release_warranty(db, &claim, actor).await?;
        }
        claim.into_claim(today())
    })
    .await
}

/// Sends the item on to a supplier or maker.
pub(crate) async fn send_to_supplier(
    db: &Db,
    key: &str,
    body: SendToSupplierRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_sent_to_supplier", async move {
        let today = today();
        if let Some(expected_back) = &body.expected_back_on
            && requested_day(expected_back, "The expected return day")? < today
        {
            return Err(AppError::validation(
                "The expected return day cannot be in the past.",
            ));
        }
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::SendToSupplier)?;
        let supplier = suppliers::service::get_supplier_by_key(db, &body.supplier_key).await?;
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.supplier = Some(SupplierClaim {
            supplier_key: supplier.key,
            supplier_name: supplier.name.clone(),
            sent_on: today.to_string(),
            reference: optional_text(body.reference),
            expected_back_on: body.expected_back_on,
            received_back_on: None,
            outcome: None,
            note: None,
        });
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::SupplierSent,
            Some(from),
            Some(format!("Sent to {}", supplier.name)),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today)
    })
    .await
}

/// Records that the item came back from the supplier, and what they did.
pub(crate) async fn supplier_return(
    db: &Db,
    key: &str,
    body: SupplierReturnRequest,
    actor: &WarrantyActor,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_back_from_supplier", async move {
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::SupplierReturn)?;
        let note = optional_text(body.note);
        let from = claim.status;
        let expected = claim.version;
        let Some(supplier) = claim.supplier.as_mut() else {
            return Err(AppError::internal(
                "a claim at the supplier always records the supplier",
            ));
        };
        supplier.received_back_on = Some(today().to_string());
        supplier.outcome = Some(body.outcome);
        supplier.note = note.clone();
        let supplier_name = supplier.supplier_name.clone();
        claim.status = to;
        save_claim(db, &mut claim, expected).await?;
        let outcome = serde_json::to_value(body.outcome)
            .expect("a supplier outcome serializes")
            .as_str()
            .expect("a supplier outcome serializes to text")
            .to_string();
        log_step(
            db,
            &claim,
            WarrantyEventKind::SupplierReturned,
            Some(from),
            Some(match note {
                Some(note) => format!("Back from {supplier_name} ({outcome}). {note}"),
                None => format!("Back from {supplier_name} ({outcome})"),
            }),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}

/// Hands the item back to the customer once the claim is finished.
pub(crate) async fn return_to_customer(
    db: &Db,
    key: &str,
    actor: &WarrantyActor,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_returned", async move {
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::ReturnToCustomer)?;
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.returned_on = Some(today().to_string());
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ReturnedToCustomer,
            Some(from),
            None,
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}

/// One claim.
pub(crate) async fn get_claim(db: &Db, key: &str) -> AppResult<WarrantyClaim> {
    require_claim(db, key).await?.into_claim(today())
}

fn statuses_for(status: ClaimListStatus) -> Option<&'static [ClaimStatus]> {
    use ClaimStatus as S;
    match status {
        ClaimListStatus::All => None,
        ClaimListStatus::Open => Some(&[S::Received, S::Inspected, S::Decided, S::AtSupplier]),
        ClaimListStatus::Received => Some(&[S::Received]),
        ClaimListStatus::Inspected => Some(&[S::Inspected]),
        ClaimListStatus::Decided => Some(&[S::Decided]),
        ClaimListStatus::AtSupplier => Some(&[S::AtSupplier]),
        ClaimListStatus::Resolved => Some(&[S::Resolved]),
        ClaimListStatus::Returned => Some(&[S::Returned]),
    }
}

/// A page of claims, oldest first, narrowed by status, warranty, invoice, supplier, technician
/// or text.
pub(crate) async fn list_claims(db: &Db, query: ClaimListQuery) -> AppResult<ClaimListResponse> {
    let text = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let filter = ClaimFilter {
        statuses: statuses_for(query.status),
        warranty_key: query.warranty_key.as_deref(),
        invoice_key: query.invoice_key.as_deref(),
        supplier_key: query.supplier_key.as_deref(),
        assigned_employee_key: query.assigned_employee_key.as_deref(),
        text,
    };
    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(LIST_LIMIT_DEFAULT)
        .clamp(1, LIST_LIMIT_MAX);
    let (items, total) = repository::list_claims(db, &filter, (page - 1) * limit, limit).await?;
    let today = today();
    Ok(ClaimListResponse {
        items: items
            .into_iter()
            .map(|claim| claim.into_claim(today))
            .collect::<AppResult<_>>()?,
        total,
        page,
        limit,
    })
}

/// Every claim on one warranty, oldest first.
pub(crate) async fn claims_for_warranty(
    db: &Db,
    warranty_key: &str,
) -> AppResult<Vec<WarrantyClaim>> {
    let filter = ClaimFilter {
        warranty_key: Some(warranty_key),
        ..ClaimFilter::default()
    };
    let today = today();
    repository::list_claims(db, &filter, 0, LINKED_CLAIMS_MAX)
        .await?
        .0
        .into_iter()
        .map(|claim| claim.into_claim(today))
        .collect()
}

/// Turns raw `warranty_claims` documents read by the sync scan into the REST shape.
pub(crate) fn hydrate_sync_claims(documents: Vec<Document>) -> AppResult<Vec<WarrantyClaim>> {
    let today = today();
    documents
        .into_iter()
        .map(|document| {
            mongodb::bson::deserialize_from_document::<WarrantyClaimDocument>(document)?
                .into_claim(today)
        })
        .collect()
}
