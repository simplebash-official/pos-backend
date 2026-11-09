//! Swapping a faulty unit for a new one at the counter in a single request. It runs the same
//! steps a claim goes through one by one (open, inspect, decide, finish, hand back) and adds no
//! rule of its own: what can be checked without writing is checked first, so a wrong PIN or an
//! unusable replacement leaves nothing behind. Each step still saves by itself, so a failure
//! after the claim is opened (stock taken by someone else in between) leaves an ordinary claim
//! at the step it reached, which can be finished from the Warranties page.

use super::claim_outcomes::{plan_replacement, resolve_claim};
use super::claim_steps::{
    decide_claim, inspect_claim, open_claim, required_text, return_to_customer, today,
};
use super::warranty_events::WarrantyActor;
use super::warranty_lookup::require;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::warranties::{
    ClaimDecision, DecideClaimRequest, InspectClaimRequest, OpenClaimRequest, QuickReplaceRequest,
    ResolveClaimRequest, WarrantyClaim, WarrantyState,
};
use crate::modules::users;

/// Opens, inspects, decides as a replacement, finishes and hands back one claim, and returns it.
/// A warranty that has run out is refused (a manager honours it through the normal claim), and so
/// is one that cannot be replaced from stock.
pub(crate) async fn quick_replace(
    db: &Db,
    body: QuickReplaceRequest,
    actor: &WarrantyActor,
    device_id: Option<String>,
) -> AppResult<WarrantyClaim> {
    crate::core::logging::domain::tracked("warranties.claim_quick_replaced", async move {
        // Everything that can be refused without writing is refused first.
        let inspection_notes = required_text(&body.inspection_notes, "what the inspection found")?;
        users::service::require_manager_approval(db, &body.approval).await?;
        let warranty = require(db, &body.warranty_key).await?;
        // A warranty that is not in force is refused by `open_claim` before it writes anything,
        // with the reason the person needs, so the replacement is only worth checking for one
        // that is.
        let state = warranty.clone().into_warranty(today())?.state;
        if matches!(state, WarrantyState::Active | WarrantyState::ExpiringSoon) {
            let old_serial = body
                .serial_number
                .as_deref()
                .or(warranty.serial_number.as_deref());
            plan_replacement(
                db,
                warranty.product_key.as_deref(),
                old_serial,
                &warranty.invoice_key,
                &body.replacement,
            )
            .await?;
        }

        let claim = open_claim(
            db,
            OpenClaimRequest {
                warranty_key: body.warranty_key,
                issue_description: body.issue_description,
                void_checks: body.void_checks,
                conditions: body.conditions,
                quantity: body.quantity,
                serial_number: body.serial_number,
                assigned_employee_key: None,
                approval: None,
            },
            actor,
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
            decide_claim(
                db,
                &claim.key,
                DecideClaimRequest {
                    decision: ClaimDecision::Replace,
                    approval: body.approval,
                },
                actor,
            )
            .await?;
            resolve_claim(
                db,
                &claim.key,
                ResolveClaimRequest {
                    resolution_note: body.resolution_note,
                    replacement: Some(body.replacement),
                },
                actor,
                device_id,
            )
            .await?;
            return_to_customer(db, &claim.key, actor).await
        }
        .await;
        finished.map_err(|error| after_open(error, &claim.claim_number))
    })
    .await
}

/// Says that a step after the claim was opened failed and where the claim is, keeping the status,
/// code and details of the failure itself.
fn after_open(error: AppError, claim_number: &str) -> AppError {
    let parts = error.wire_parts();
    let message = format!(
        "{} The claim {claim_number} was started and is waiting on the Warranties page.",
        parts.message
    );
    match parts.details {
        Some(details) => AppError::custom_with_details(parts.status, parts.code, message, details),
        None => AppError::custom(parts.status, parts.code, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::constants::codes;
    use axum::http::StatusCode;

    #[test]
    fn a_failure_after_the_claim_opened_names_the_claim_and_keeps_its_status_and_code() {
        let error = AppError::conflict(codes::CLAIM_REPLACEMENT_INVALID, "No unit in stock.");
        let parts = after_open(error, "WCL-000007").wire_parts();
        assert_eq!(parts.status, StatusCode::CONFLICT);
        assert_eq!(parts.code, codes::CLAIM_REPLACEMENT_INVALID);
        assert_eq!(
            parts.message,
            "No unit in stock. The claim WCL-000007 was started and is waiting on the Warranties page."
        );
        assert!(parts.details.is_none());
    }

    #[test]
    fn the_details_of_the_failure_are_kept() {
        let error = AppError::conflict_with_details(
            codes::VERSION_CONFLICT,
            "Changed just now.",
            serde_json::json!({ "key": "wcl_1" }),
        );
        let parts = after_open(error, "WCL-000008").wire_parts();
        assert_eq!(parts.details, Some(serde_json::json!({ "key": "wcl_1" })));
    }
}
