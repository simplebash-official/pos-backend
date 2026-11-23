//! Taking a faulty item in, inspecting it, deciding it and carrying out the decision's outcome
//! in a single request. It runs the same steps a claim goes through one by one (open, inspect,
//! decide, then resolve and hand back as far as the outcome goes) and adds no rule of its own:
//! what can be checked without writing is checked first, so a wrong PIN or an unusable
//! replacement leaves nothing behind. Each step still saves by itself, so a failure after the
//! claim is opened (stock taken by someone else in between) leaves an ordinary claim at the step
//! it reached, which can be carried on from the Warranties page.

use super::claim_outcomes::{plan_replacement, resolve_claim};
use super::claim_steps::{
    decide_claim, inspect_claim, open_claim, required_text, return_to_customer, today,
};
use super::warranty_events::WarrantyActor;
use super::warranty_lookup::require;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::users::ApprovalCaller;
use crate::domain::warranties::{
    ClaimDecision, DecideClaimRequest, InspectClaimRequest, OpenClaimRequest, ProcessClaimRequest,
    ProcessOutcome, RepairFinish, ResolveClaimRequest, WarrantyClaim, WarrantyState,
};
use crate::modules::users;
use serde_json::{Map, Value, json};

/// Opens, inspects and decides one claim, carries out the outcome (finishing and handing back
/// where the outcome says so), and returns the claim as it stands. A warranty that has run out
/// is honoured with the caller's approval, exactly as `open_claim` does it.
pub(crate) async fn process_claim(
    db: &Db,
    body: ProcessClaimRequest,
    actor: &WarrantyActor,
    caller: &ApprovalCaller,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_processed", async move {
        let ProcessClaimRequest {
            warranty_key,
            issue_description,
            inspection_notes,
            void_checks,
            conditions,
            quantity,
            serial_number,
            outcome,
            approval,
        } = body;
        // Everything that can be refused without writing is refused first.
        let inspection_notes = required_text(&inspection_notes, "what the inspection found")?;
        users::service::resolve_approval(db, caller, approval.as_ref()).await?;
        let warranty = require(db, &warranty_key).await?;
        let state = warranty.clone().into_warranty(today())?.state;
        // A cancelled warranty or one already under a claim is refused by `open_claim` before it
        // writes anything, with the reason the person needs; the replacement is only worth
        // checking for a warranty that can be claimed.
        if let ProcessOutcome::Replace { replacement, .. } = &outcome
            && matches!(
                state,
                WarrantyState::Active | WarrantyState::ExpiringSoon | WarrantyState::Expired
            )
        {
            let old_serial = serial_number
                .as_deref()
                .or(warranty.serial_number.as_deref());
            plan_replacement(
                db,
                warranty.product_key.as_deref(),
                old_serial,
                &warranty.invoice_key,
                replacement,
            )
            .await?;
        }

        // `open_claim` takes an approval only for a warranty that has run out.
        let open_approval = if state == WarrantyState::Expired {
            approval.clone()
        } else {
            None
        };
        let claim = open_claim(
            db,
            OpenClaimRequest {
                warranty_key,
                issue_description,
                void_checks,
                conditions,
                quantity,
                serial_number,
                assigned_employee_key: None,
                approval: open_approval,
            },
            actor,
            caller,
            device_id.clone(),
        )
        .await?;
        let finished = async {
            inspect_claim(
                db,
                &claim.key,
                InspectClaimRequest {
                    inspection_notes,
                    assigned_employee_key: None,
                },
                actor,
            )
            .await?;
            let decision = match &outcome {
                ProcessOutcome::Replace { .. } => ClaimDecision::Replace,
                ProcessOutcome::Repair { .. } => ClaimDecision::Repair,
                ProcessOutcome::Refund => ClaimDecision::Refund,
                ProcessOutcome::Reject { .. } => ClaimDecision::Reject,
            };
            let decided = decide_claim(
                db,
                &claim.key,
                DecideClaimRequest { decision, approval },
                actor,
                caller,
            )
            .await?;
            match outcome {
                ProcessOutcome::Replace {
                    replacement,
                    resolution_note,
                } => {
                    resolve_claim(
                        db,
                        &claim.key,
                        ResolveClaimRequest {
                            resolution_note,
                            replacement: Some(replacement),
                        },
                        actor,
                        device_id,
                    )
                    .await?;
                    return_to_customer(db, &claim.key, actor).await
                }
                ProcessOutcome::Repair {
                    finish: RepairFinish::Now { resolution_note },
                } => {
                    resolve_claim(
                        db,
                        &claim.key,
                        ResolveClaimRequest {
                            resolution_note,
                            replacement: None,
                        },
                        actor,
                        device_id,
                    )
                    .await?;
                    return_to_customer(db, &claim.key, actor).await
                }
                ProcessOutcome::Reject { hand_back: true } => {
                    return_to_customer(db, &claim.key, actor).await
                }
                ProcessOutcome::Repair {
                    finish: RepairFinish::Later,
                }
                | ProcessOutcome::Refund
                | ProcessOutcome::Reject { hand_back: false } => Ok(decided),
            }
        }
        .await;
        finished.map_err(|error| after_open(error, &claim.key, &claim.claim_number))
    })
    .await
}

/// Says that a step after the claim was opened failed and where the claim is, keeping the status
/// and code of the failure itself, and names the claim in the details so the screen can open it.
fn after_open(error: AppError, claim_key: &str, claim_number: &str) -> AppError {
    let parts = error.wire_parts();
    let message = format!(
        "{} The claim {claim_number} was started and is waiting on the Warranties page.",
        parts.message
    );
    let mut details = match parts.details {
        Some(Value::Object(map)) => map,
        Some(other) => Map::from_iter([("cause".to_string(), other)]),
        None => Map::new(),
    };
    details.insert("claimKey".to_string(), json!(claim_key));
    details.insert("claimNumber".to_string(), json!(claim_number));
    AppError::custom_with_details(parts.status, parts.code, message, Value::Object(details))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::constants::codes;
    use axum::http::StatusCode;

    #[test]
    fn a_failure_after_the_claim_opened_names_the_claim_and_keeps_its_status_and_code() {
        let error = AppError::conflict(codes::CLAIM_REPLACEMENT_INVALID, "No unit in stock.");
        let parts = after_open(error, "wcl_7", "WCL-000007").wire_parts();
        assert_eq!(parts.status, StatusCode::CONFLICT);
        assert_eq!(parts.code, codes::CLAIM_REPLACEMENT_INVALID);
        assert_eq!(
            parts.message,
            "No unit in stock. The claim WCL-000007 was started and is waiting on the Warranties page."
        );
        assert_eq!(
            parts.details,
            Some(serde_json::json!({ "claimKey": "wcl_7", "claimNumber": "WCL-000007" }))
        );
    }

    #[test]
    fn the_details_of_the_failure_are_kept() {
        let error = AppError::conflict_with_details(
            codes::VERSION_CONFLICT,
            "Changed just now.",
            serde_json::json!({ "key": "wcl_1" }),
        );
        let parts = after_open(error, "wcl_8", "WCL-000008").wire_parts();
        assert_eq!(
            parts.details,
            Some(serde_json::json!({
                "key": "wcl_1",
                "claimKey": "wcl_8",
                "claimNumber": "WCL-000008"
            }))
        );
    }

    #[test]
    fn a_rejection_must_say_whether_it_is_handed_back() {
        let missing = serde_json::from_value::<ProcessOutcome>(json!({ "decision": "reject" }));
        assert!(missing.is_err());
        let given = serde_json::from_value::<ProcessOutcome>(
            json!({ "decision": "reject", "handBack": false }),
        );
        assert!(matches!(
            given,
            Ok(ProcessOutcome::Reject { hand_back: false })
        ));
    }

    #[test]
    fn a_repair_must_say_whether_it_is_finished_now() {
        let missing = serde_json::from_value::<ProcessOutcome>(json!({ "decision": "repair" }));
        assert!(missing.is_err());
        let later = serde_json::from_value::<ProcessOutcome>(
            json!({ "decision": "repair", "finish": { "when": "later" } }),
        );
        assert!(matches!(
            later,
            Ok(ProcessOutcome::Repair {
                finish: RepairFinish::Later
            })
        ));
    }
}
