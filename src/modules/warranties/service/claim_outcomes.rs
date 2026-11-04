//! What a decided claim does: a repair puts the warranty back in force, a replacement takes a
//! new unit out of stock with a warranty that runs to the old end date, a refund ends with a
//! credit note, and a rejected item can become a paid repair job.

use super::claim_steps::{
    log_step, release_warranty, require_claim, save_claim, save_warranty, step, today,
};
use super::warranty_events::{self, NewEvent, WarrantyActor};
use super::warranty_issue::next_number;
use super::warranty_lookup::require;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::{BatchAllocation, StockMovementType};
use crate::domain::repairs::{CreateRepairRequest, RepairCustomer};
use crate::domain::warranties::{
    ClaimAction, ClaimDecision, ClaimStatus, ReplacementRequest, ResolveClaimRequest,
    WarrantyClaim, WarrantyEventKind, WarrantyStatus,
};
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::warranties::model::{WarrantyClaimDocument, WarrantyDocument};
use crate::modules::warranties::repository;
use crate::modules::{inventory, repairs};
use mongodb::bson::DateTime as BsonDateTime;

fn replacement_invalid(message: impl Into<String>) -> AppError {
    AppError::conflict(codes::CLAIM_REPLACEMENT_INVALID, message.into())
}

/// The new unit, checked and its stock planned, before anything is written.
#[derive(Debug)]
struct PlannedReplacement {
    product_key: String,
    /// The faulty unit coming back and the in-stock unit going out, for a serialized item.
    serials: Option<(ProductSerialDocument, ProductSerialDocument)>,
    allocations: Vec<BatchAllocation>,
}

async fn plan_replacement(
    db: &Db,
    claim: &WarrantyClaimDocument,
    replacement: &ReplacementRequest,
) -> AppResult<PlannedReplacement> {
    let Some(product_key) = claim.product_key.clone() else {
        return Err(replacement_invalid(
            "Only an item sold from stock can be replaced from stock.",
        ));
    };
    let product = inventory::service::product::get_product_by_key(db, &product_key).await?;
    match (&claim.serial_number, replacement) {
        (
            Some(old_serial),
            ReplacementRequest {
                serial_number: Some(new_serial),
                selling_price_cents: None,
            },
        ) => {
            if new_serial == old_serial {
                return Err(replacement_invalid(
                    "Scan the new unit, not the faulty one.",
                ));
            }
            let outgoing = inventory::service::product_serial::resolve_in_stock_serial(
                db,
                &product_key,
                new_serial,
            )
            .await?;
            let incoming = inventory::service::product_serial::resolve_sold_serial_for_invoice(
                db,
                &product_key,
                old_serial,
                &claim.invoice_key,
            )
            .await?;
            let (_, allocations) = inventory::service::stock_batch::allocations_for_serials(
                db,
                &product_key,
                &product.name,
                std::slice::from_ref(&outgoing.batch_key),
            )
            .await?;
            Ok(PlannedReplacement {
                product_key,
                serials: Some((incoming, outgoing)),
                allocations,
            })
        }
        (Some(_), _) => Err(replacement_invalid(
            "Scan the serial number of the new unit.",
        )),
        (
            None,
            ReplacementRequest {
                serial_number: None,
                selling_price_cents: Some(price),
            },
        ) => {
            if product.is_serialized {
                return Err(replacement_invalid(
                    "This item now tracks serial numbers; it cannot be replaced by price.",
                ));
            }
            let allocations = inventory::service::stock_batch::plan_tier_allocation(
                db,
                &product_key,
                &product.name,
                *price,
                1,
            )
            .await?;
            Ok(PlannedReplacement {
                product_key,
                serials: None,
                allocations,
            })
        }
        (None, _) => Err(replacement_invalid(
            "Choose which price the new unit is taken from.",
        )),
    }
}

/// Lets go of one unit of a warranty that a claim used up (replaced or refunded): a warranty
/// on several units keeps the rest and goes back in force; one on a single unit is cancelled.
async fn give_up_unit(
    db: &Db,
    claim: &WarrantyClaimDocument,
    replaced_by: Option<&str>,
    reason: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let mut warranty = require(db, &claim.warranty_key).await?;
    if warranty.status == WarrantyStatus::Void {
        return Ok(());
    }
    let from = warranty.status;
    let expected = warranty.version;
    if warranty.quantity > 1 {
        warranty.quantity -= 1;
        warranty.status = WarrantyStatus::Active;
    } else {
        warranty.status = WarrantyStatus::Void;
        warranty.void_reason = Some(reason.to_string());
    }
    if let Some(key) = replaced_by {
        warranty.replaced_by_warranty_key = Some(key.to_string());
    }
    save_warranty(db, &mut warranty, expected).await?;
    warranty_events::record(
        db,
        NewEvent {
            warranty_key: &warranty.key,
            claim_key: Some(&claim.key),
            kind: if replaced_by.is_some() {
                WarrantyEventKind::Replaced
            } else {
                WarrantyEventKind::Voided
            },
            from_status: Some(from.as_str()),
            to_status: Some(warranty.status.as_str()),
            note: Some(reason.to_string()),
            actor,
            approved_by: None,
        },
    )
    .await
}

/// The new unit's warranty: the old one's cover, terms and price, starting today and ending on
/// the old end date.
fn replacement_warranty(
    old: &WarrantyDocument,
    key: String,
    number: String,
    serial_number: Option<String>,
) -> WarrantyDocument {
    let now = BsonDateTime::now();
    WarrantyDocument {
        id: None,
        key,
        warranty_number: number,
        serial_number,
        quantity: 1,
        starts_on: today().to_string(),
        status: WarrantyStatus::Active,
        void_reason: None,
        replaces_warranty_key: Some(old.key.clone()),
        replaced_by_warranty_key: None,
        claim_count: 0,
        version: 1,
        created_at: now,
        updated_at: now,
        updated_by_device: None,
        ..old.clone()
    }
}

/// Finishes a claim decided as a repair or a replacement. A repair puts the warranty back in
/// force with its old end date. A replacement takes the new unit out of stock, sells it on the
/// original invoice, takes the faulty serial back as `returned_faulty`, and gives the new unit
/// a warranty that runs to the old end date.
pub(crate) async fn resolve_claim(
    db: &Db,
    key: &str,
    body: ResolveClaimRequest,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_resolved", async move {
        let mut claim = require_claim(db, key).await?;
        let to = step(&claim, ClaimAction::Resolve)?;
        let note = body
            .resolution_note
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        let decision = claim
            .decision
            .ok_or_else(|| AppError::internal("a decided claim records its decision"))?;
        let planned = match (decision, &body.replacement) {
            (ClaimDecision::Repair, None) => None,
            (ClaimDecision::Repair, Some(_)) => {
                return Err(AppError::validation(
                    "A repaired item keeps its own unit; leave the replacement out.",
                ));
            }
            (ClaimDecision::Replace, Some(replacement)) => {
                Some(plan_replacement(db, &claim, replacement).await?)
            }
            (ClaimDecision::Replace, None) => {
                return Err(replacement_invalid("Choose the new unit to give."));
            }
            (ClaimDecision::Refund | ClaimDecision::Reject, _) => {
                return Err(AppError::internal(
                    "the claim lifecycle only resolves repairs and replacements",
                ));
            }
        };
        let old_warranty = require(db, &claim.warranty_key).await?;
        let new_warranty = match &planned {
            Some(_) => Some((
                generate_id(prefixes::WARRANTY),
                next_number(db, "warranty", device_id).await?,
            )),
            None => None,
        };

        // The claim is written first: its version decides which of two racing requests wins,
        // before any stock moves.
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.resolution_note = note.clone();
        claim.resolved_on = Some(today().to_string());
        if let (Some(plan), Some((warranty_key, _))) = (&planned, &new_warranty) {
            claim.replacement_serial_number = plan
                .serials
                .as_ref()
                .map(|(_, out)| out.serial_number.clone());
            claim.replacement_warranty_key = Some(warranty_key.clone());
        }
        save_claim(db, &mut claim, expected).await?;

        match (planned, new_warranty) {
            (Some(plan), Some((warranty_key, warranty_number))) => {
                let reason = format!("Replaced under {}", claim.claim_number);
                inventory::service::stock_batch::move_allocations(
                    db,
                    &plan.product_key,
                    &plan.allocations,
                    -1,
                    StockMovementType::WarrantyReplacement,
                    &claim.key,
                    &reason,
                )
                .await?;
                let new_serial = match &plan.serials {
                    Some((incoming, outgoing)) => {
                        let serial_id = |serial: &ProductSerialDocument| {
                            serial.id.ok_or_else(|| {
                                AppError::internal("persisted product serial has an id")
                            })
                        };
                        inventory::service::product_serial::mark_serial_sold(
                            db,
                            serial_id(outgoing)?,
                            &claim.invoice_key,
                        )
                        .await?;
                        inventory::service::product_serial::mark_serial_returned_faulty(
                            db,
                            serial_id(incoming)?,
                        )
                        .await?;
                        Some(outgoing.serial_number.clone())
                    }
                    None => None,
                };
                let inserted = repository::insert_warranty(
                    db,
                    replacement_warranty(&old_warranty, warranty_key, warranty_number, new_serial),
                )
                .await?;
                warranty_events::record(
                    db,
                    NewEvent {
                        warranty_key: &inserted.key,
                        claim_key: Some(&claim.key),
                        kind: WarrantyEventKind::Created,
                        from_status: None,
                        to_status: Some(WarrantyStatus::Active.as_str()),
                        note: Some(format!(
                            "Replacement for {} under {}",
                            old_warranty.warranty_number, claim.claim_number
                        )),
                        actor,
                        approved_by: None,
                    },
                )
                .await?;
                give_up_unit(db, &claim, Some(&inserted.key), &reason, actor).await?;
            }
            _ => release_warranty(db, &claim, actor).await?,
        }
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimAdvanced,
            Some(from),
            note,
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}

/// Checks, before a credit note is written, that it may finish this claim: the claim was
/// decided as a refund, it is on the same invoice, and a serialized item is among what comes
/// back.
pub(crate) async fn check_claim_refund(
    db: &Db,
    claim_key: &str,
    invoice_key: &str,
    returned_serials: &[&str],
) -> AppResult<()> {
    let claim = require_claim(db, claim_key).await?;
    step(&claim, ClaimAction::Refunded)?;
    if claim.invoice_key != invoice_key {
        return Err(AppError::validation(format!(
            "Claim {} is for invoice {}; refund it from that invoice.",
            claim.claim_number, claim.invoice_number
        )));
    }
    if let Some(serial) = &claim.serial_number
        && !returned_serials.contains(&serial.as_str())
    {
        return Err(AppError::validation(format!(
            "Return the claimed unit ({serial}) on this credit note."
        )));
    }
    Ok(())
}

/// Finishes a refund claim once its credit note exists: the claim is resolved and linked to
/// the credit note, and the claimed unit's warranty ends.
pub(crate) async fn finish_claim_refund(
    db: &Db,
    claim_key: &str,
    credit_note_key: &str,
    credit_note_number: &str,
    actor: &WarrantyActor,
) -> AppResult<()> {
    crate::core::logging::domain::tracked("warranties.claim_refunded", async move {
        let mut claim = require_claim(db, claim_key).await?;
        let to = step(&claim, ClaimAction::Refunded)?;
        let from = claim.status;
        let expected = claim.version;
        claim.status = to;
        claim.credit_note_key = Some(credit_note_key.to_string());
        claim.resolved_on = Some(today().to_string());
        save_claim(db, &mut claim, expected).await?;
        let reason = format!("Refunded on {credit_note_number}");
        give_up_unit(db, &claim, None, &reason, actor).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimAdvanced,
            Some(from),
            Some(reason),
            actor,
            None,
        )
        .await
    })
    .await
}

/// Opens a paid repair job for an item whose claim was rejected, so the customer can still
/// have it fixed. Once per claim.
pub(crate) async fn convert_to_paid_repair(
    db: &Db,
    key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_paid_repair", async move {
        let mut claim = require_claim(db, key).await?;
        let rejected_and_done = claim.decision == Some(ClaimDecision::Reject)
            && matches!(claim.status, ClaimStatus::Resolved | ClaimStatus::Returned);
        if !rejected_and_done {
            return Err(AppError::conflict(
                codes::CLAIM_STEP_NOT_ALLOWED,
                "Only a rejected claim can become a paid repair.",
            ));
        }
        if let Some(number) = &claim.paid_repair_number {
            return Err(AppError::conflict(
                codes::CLAIM_STEP_NOT_ALLOWED,
                format!("This claim already has repair job {number}."),
            ));
        }
        let reason = claim
            .decision_reason
            .clone()
            .ok_or_else(|| AppError::internal("a decided claim records why it was decided"))?;
        let repair = repairs::service::create_repair(
            db,
            CreateRepairRequest {
                customer: RepairCustomer {
                    customer_key: claim.customer_key.clone(),
                    customer_name: claim.customer_name.clone(),
                    customer_phone: claim.customer_phone.clone(),
                },
                device_model: claim.item_name.clone(),
                serial_number: claim.serial_number.clone(),
                issue_description: format!(
                    "{} (warranty claim {} not covered: {reason})",
                    claim.issue_description, claim.claim_number
                ),
                promised_ready_at: None,
                status: None,
                items: Vec::new(),
                assignment: None,
            },
            device_id,
        )
        .await?;
        let expected = claim.version;
        claim.paid_repair_key = Some(repair.key);
        claim.paid_repair_number = Some(repair.ticket_number.clone());
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimAdvanced,
            None,
            Some(format!("Paid repair {} opened", repair.ticket_number)),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}

/// Opens the free rework job for a claim on an earlier repair that was decided as a repair:
/// same customer and device, linked back to the claim and the original job, charging nothing.
/// Once per claim, and only before the claim is resolved.
pub(crate) async fn open_rework(
    db: &Db,
    key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_rework", async move {
        let mut claim = require_claim(db, key).await?;
        if claim.decision != Some(ClaimDecision::Repair) || claim.status != ClaimStatus::Decided {
            return Err(AppError::conflict(
                codes::CLAIM_STEP_NOT_ALLOWED,
                "A rework job is opened once a claim is decided as a repair, before it is resolved.",
            ));
        }
        if let Some(number) = &claim.rework_repair_number {
            return Err(AppError::conflict(
                codes::CLAIM_STEP_NOT_ALLOWED,
                format!("This claim already has rework job {number}."),
            ));
        }
        let warranty = require(db, &claim.warranty_key).await?;
        let Some(original_repair_key) = warranty.repair_key.as_deref() else {
            return Err(AppError::conflict(
                codes::CLAIM_REWORK_INVALID,
                "Only a warranty on repair work can be reworked; this one came with a product.",
            ));
        };
        let rework = repairs::service::create_rework(
            db,
            repairs::service::NewRework {
                claim_key: &claim.key,
                claim_number: &claim.claim_number,
                original_repair_key,
                issue_description: &claim.issue_description,
                assigned_employee_key: claim.assigned_employee_key.as_deref(),
                device_id,
            },
        )
        .await?;
        let expected = claim.version;
        claim.rework_repair_key = Some(rework.key);
        claim.rework_repair_number = Some(rework.ticket_number.clone());
        save_claim(db, &mut claim, expected).await?;
        log_step(
            db,
            &claim,
            WarrantyEventKind::ClaimAdvanced,
            None,
            Some(format!("Rework job {} opened", rework.ticket_number)),
            actor,
            None,
        )
        .await?;
        claim.into_claim(today())
    })
    .await
}
