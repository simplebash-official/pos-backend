//! What a decided claim does: a repair puts the warranty back in force, a replacement takes a
//! new unit out of stock with a warranty that runs to the old end date, a refund ends with a
//! credit note, a rejected item can become a paid repair or print job, and repair or print work
//! decided as a repair opens a free rework job.

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
use crate::domain::billing::CreateCreditNoteItemRequest;
use crate::domain::inventory::{BatchAllocation, StockMovementType};
use crate::domain::repairs::{CreateRepairRequest, RepairCustomer};
use crate::domain::warranties::{
    ClaimAction, ClaimDecision, ClaimStatus, PaidJobRequest, ReplacementRequest,
    ResolveClaimRequest, WarrantyClaim, WarrantyEventKind, WarrantySourceType, WarrantyStatus,
};
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::warranties::model::{WarrantyClaimDocument, WarrantyDocument};
use crate::modules::warranties::repository;
use crate::modules::{inventory, print_jobs, repairs};
use mongodb::bson::DateTime as BsonDateTime;

fn replacement_invalid(message: impl Into<String>) -> AppError {
    AppError::conflict(codes::CLAIM_REPLACEMENT_INVALID, message.into())
}

/// The new unit, checked and its stock planned, before anything is written.
#[derive(Debug)]
pub(super) struct PlannedReplacement {
    product_key: String,
    /// The faulty unit coming back and the in-stock unit going out, for a serialized item.
    serials: Option<(ProductSerialDocument, ProductSerialDocument)>,
    allocations: Vec<BatchAllocation>,
}

pub(super) async fn plan_replacement(
    db: &Db,
    product_key: Option<&str>,
    old_serial: Option<&str>,
    invoice_key: &str,
    replacement: &ReplacementRequest,
) -> AppResult<PlannedReplacement> {
    let Some(product_key) = product_key.map(str::to_string) else {
        return Err(replacement_invalid(
            "Only an item sold from stock can be replaced from stock.",
        ));
    };
    let product = inventory::service::product::get_product_by_key(db, &product_key).await?;
    match (old_serial, replacement) {
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
                invoice_key,
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
    let claim_qty = claim.quantity.unwrap_or(1);
    warranty.claimed_quantity = Some(
        warranty
            .claimed_quantity
            .unwrap_or(claim_qty)
            .saturating_sub(claim_qty),
    );
    if warranty.quantity > claim_qty {
        warranty.quantity -= claim_qty;
        warranty.status = WarrantyStatus::Active;
    } else {
        warranty.quantity = 0;
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
        claimed_quantity: Some(0),
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
            (ClaimDecision::Replace, Some(replacement)) => Some(
                plan_replacement(
                    db,
                    claim.product_key.as_deref(),
                    claim.serial_number.as_deref(),
                    &claim.invoice_key,
                    replacement,
                )
                .await?,
            ),
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
/// decided as a refund, it is on the same invoice, and the returned items contain only
/// the single claimed item with the exact claimed quantity (or serialized unit).
pub(crate) async fn check_claim_refund(
    db: &Db,
    claim_key: &str,
    invoice_key: &str,
    returned_items: &[CreateCreditNoteItemRequest],
) -> AppResult<()> {
    let claim = require_claim(db, claim_key).await?;
    step(&claim, ClaimAction::Refunded)?;
    if claim.invoice_key != invoice_key {
        return Err(AppError::validation(format!(
            "Claim {} is for invoice {}; refund it from that invoice.",
            claim.claim_number, claim.invoice_number
        )));
    }
    if returned_items.len() != 1 {
        return Err(AppError::validation(format!(
            "Warranty claim refund on {} can only return the single claimed item line.",
            claim.claim_number
        )));
    }
    let returned = &returned_items[0];

    if let Some(serial) = &claim.serial_number {
        if returned.serial_number.as_deref() != Some(serial.as_str()) {
            return Err(AppError::validation(format!(
                "Return the claimed unit ({serial}) on this credit note."
            )));
        }
        if returned.quantity != 1 {
            return Err(AppError::validation(format!(
                "Serialized warranty claim {} must return exactly 1 unit.",
                claim.claim_number
            )));
        }
    } else {
        let expected_qty = claim.quantity.unwrap_or(1);
        if returned.quantity != expected_qty {
            return Err(AppError::validation(format!(
                "Warranty claim {} is for {} unit(s); return quantity must be exactly {}.",
                claim.claim_number, expected_qty, expected_qty
            )));
        }
        if let Some(expected_prod) = &claim.product_key
            && let Some(returned_prod) = &returned.product_key
            && expected_prod != returned_prod
        {
            return Err(AppError::validation(format!(
                "Returned item does not match claimed item '{}'.",
                claim.item_name
            )));
        }
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

/// What the warranty a claim is on came from, which decides the kind of job it can open.
pub(crate) async fn claim_source(db: &Db, key: &str) -> AppResult<WarrantySourceType> {
    let claim = require_claim(db, key).await?;
    Ok(require(db, &claim.warranty_key).await?.source_type)
}

/// The number of the paid job a claim already has, of either kind.
fn paid_job_number(claim: &WarrantyClaimDocument) -> Option<&str> {
    claim
        .paid_repair_number
        .as_deref()
        .or(claim.paid_print_job_number.as_deref())
}

/// A rejected claim that may still become a paid job: the claim, the customer the job is for,
/// and why the claim was not covered.
#[derive(Debug)]
struct RejectedClaim {
    claim: WarrantyClaimDocument,
    customer_key: String,
    /// The customer's complaint, with the claim it came from.
    complaint: String,
}

/// Checks a claim can become a paid job: rejected, finished, and without one yet. A walk-in
/// sale leaves the claim without a customer, so the registered customer in `customer` stands in;
/// a customer the claim already has always wins. `job_kind` ("repair" or "print") words the
/// refusals.
async fn rejected_claim(
    db: &Db,
    key: &str,
    customer: PaidJobRequest,
    job_kind: &str,
) -> AppResult<RejectedClaim> {
    let claim = require_claim(db, key).await?;
    let rejected_and_done = claim.decision == Some(ClaimDecision::Reject)
        && matches!(claim.status, ClaimStatus::Resolved | ClaimStatus::Returned);
    if !rejected_and_done {
        return Err(AppError::conflict(
            codes::CLAIM_STEP_NOT_ALLOWED,
            format!("Only a rejected claim can become a paid {job_kind} job."),
        ));
    }
    if let Some(number) = paid_job_number(&claim) {
        return Err(AppError::conflict(
            codes::CLAIM_STEP_NOT_ALLOWED,
            format!("This claim already has {job_kind} job {number}."),
        ));
    }
    // A decision a signed-in manager made themselves carries no written reason.
    let not_covered = match &claim.decision_reason {
        Some(reason) => format!("not covered: {reason}"),
        None => "not covered".to_string(),
    };
    let customer_key = claim
        .customer_key
        .clone()
        .or(customer.customer_key)
        .ok_or_else(|| {
            AppError::validation(format!("Choose the customer for this {job_kind} job."))
        })?;
    let complaint = format!(
        "{} (warranty claim {} {not_covered})",
        claim.issue_description, claim.claim_number
    );
    Ok(RejectedClaim {
        claim,
        customer_key,
        complaint,
    })
}

/// Saves a claim that gained a job and logs the step.
async fn save_job_on_claim(
    db: &Db,
    claim: &mut WarrantyClaimDocument,
    note: String,
    actor: &WarrantyActor,
) -> AppResult<()> {
    let expected = claim.version;
    save_claim(db, claim, expected).await?;
    log_step(
        db,
        claim,
        WarrantyEventKind::ClaimAdvanced,
        None,
        Some(note),
        actor,
        None,
    )
    .await
}

/// Opens a paid repair job for an item whose claim was rejected, so the customer can still
/// have it fixed. Once per claim. Print work gets a paid print job instead.
pub(crate) async fn convert_to_paid_repair(
    db: &Db,
    key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
    customer: PaidJobRequest,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_paid_repair", async move {
        if claim_source(db, key).await? == WarrantySourceType::Print {
            return Err(AppError::conflict(
                codes::CLAIM_PAID_JOB_INVALID,
                "This claim is about print work: make a paid print job instead.",
            ));
        }
        let RejectedClaim {
            mut claim,
            customer_key,
            complaint,
        } = rejected_claim(db, key, customer, "repair").await?;
        let repair = repairs::service::create_repair(
            db,
            CreateRepairRequest {
                customer: RepairCustomer { customer_key },
                device_model: claim.item_name.clone(),
                serial_number: claim.serial_number.clone(),
                issue_description: complaint,
                promised_ready_at: None,
                status: None,
                items: Vec::new(),
                assignment: None,
            },
            device_id,
        )
        .await?;
        claim.paid_repair_key = Some(repair.key);
        claim.paid_repair_number = Some(repair.ticket_number.clone());
        let note = format!("Paid repair {} opened", repair.ticket_number);
        save_job_on_claim(db, &mut claim, note, actor).await?;
        claim.into_claim(today())
    })
    .await
}

/// Opens a paid print job for print work whose claim was rejected, so the customer can still
/// have it done again. Once per claim.
pub(crate) async fn convert_to_paid_print_job(
    db: &Db,
    key: &str,
    actor: &WarrantyActor,
    device_id: Option<String>,
    customer: PaidJobRequest,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_paid_print_job", async move {
        let warranty_key = require_claim(db, key).await?.warranty_key;
        let warranty = require(db, &warranty_key).await?;
        let Some(original_print_job_key) = warranty.print_job_key.as_deref() else {
            return Err(AppError::conflict(
                codes::CLAIM_PAID_JOB_INVALID,
                "Only a claim about print work can become a paid print job.",
            ));
        };
        let RejectedClaim {
            mut claim,
            customer_key,
            complaint,
        } = rejected_claim(db, key, customer, "print").await?;
        let job = print_jobs::service::create_paid_job(
            db,
            print_jobs::service::NewPaidJob {
                customer_key: &customer_key,
                original_print_job_key,
                note: complaint,
                device_id,
            },
        )
        .await?;
        claim.paid_print_job_key = Some(job.key);
        claim.paid_print_job_number = Some(job.ticket_number.clone());
        let note = format!("Paid print job {} opened", job.ticket_number);
        save_job_on_claim(db, &mut claim, note, actor).await?;
        claim.into_claim(today())
    })
    .await
}

/// Opens the free rework job for a claim on earlier repair or print work that was decided as a
/// repair: the same customer, linked back to the claim and the original job, charging nothing.
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
        if let Some(number) = claim
            .rework_repair_number
            .as_deref()
            .or(claim.rework_print_job_number.as_deref())
        {
            return Err(AppError::conflict(
                codes::CLAIM_STEP_NOT_ALLOWED,
                format!("This claim already has rework job {number}."),
            ));
        }
        let warranty = require(db, &claim.warranty_key).await?;
        let invalid = || {
            AppError::conflict(
                codes::CLAIM_REWORK_INVALID,
                "Only a warranty on repair or print work can be reworked; this one came with a \
                 product.",
            )
        };
        let ticket_number = match warranty.source_type {
            WarrantySourceType::ProductSale => return Err(invalid()),
            WarrantySourceType::Repair => {
                let original_repair_key = warranty.repair_key.as_deref().ok_or_else(invalid)?;
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
                claim.rework_repair_key = Some(rework.key);
                claim.rework_repair_number = Some(rework.ticket_number.clone());
                rework.ticket_number
            }
            WarrantySourceType::Print => {
                let original_print_job_key =
                    warranty.print_job_key.as_deref().ok_or_else(invalid)?;
                let rework = print_jobs::service::create_rework(
                    db,
                    print_jobs::service::NewPrintRework {
                        claim_key: &claim.key,
                        claim_number: &claim.claim_number,
                        original_print_job_key,
                        issue_description: &claim.issue_description,
                        assigned_employee_key: claim.assigned_employee_key.as_deref(),
                        device_id,
                    },
                )
                .await?;
                claim.rework_print_job_key = Some(rework.key);
                claim.rework_print_job_number = Some(rework.ticket_number.clone());
                rework.ticket_number
            }
        };
        let note = format!("Rework job {ticket_number} opened");
        save_job_on_claim(db, &mut claim, note, actor).await?;
        claim.into_claim(today())
    })
    .await
}
