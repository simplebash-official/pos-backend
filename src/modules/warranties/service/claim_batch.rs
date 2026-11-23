//! Opening several claims on one invoice in one request. It adds no claim rule of its own: each
//! item goes through the same `open_claim` as a single claim. What it owns is the request as a
//! whole (shape, one invoice, one approval check) and the per-item answer. There is no
//! transaction in the claim flow, so an item that fails never undoes one that opened.

use super::claim_steps::{open_claim_honoured, today};
use super::warranty_events::WarrantyActor;
use super::warranty_lookup::require;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::users::ApprovalCaller;
use crate::domain::warranties::{
    BatchClaimError, BatchClaimResult, OpenClaimRequest, OpenClaimsBatchRequest,
    OpenClaimsBatchResponse, WarrantyState,
};
use crate::modules::users;
use std::collections::HashSet;

/// Most claims one batch opens.
const CLAIM_BATCH_MAX: usize = 100;

/// Opens one claim per item, in the order sent. A request that is wrong as a whole (no items,
/// too many, a warranty twice or unknown, warranties of different invoices, an approval nobody
/// needs) is refused before anything is written; after that every item succeeds or fails on its
/// own and the answer lists them all.
///
/// A warranty that has run out needs a manager's or owner's approval. It is checked once for the
/// whole batch (a wrong PIN refuses the request) and honours only the items that ran out. Staff
/// who send none still open the warranties in force; the ones that ran out come back as failed.
pub(crate) async fn open_claims_batch(
    db: &Db,
    body: OpenClaimsBatchRequest,
    actor: &WarrantyActor,
    caller: &ApprovalCaller,
    device_id: Option<String>,
) -> AppResult<OpenClaimsBatchResponse> {
    crate::core::logging::domain::tracked("warranties.claims_batch_opened", async move {
        if body.items.is_empty() {
            return Err(AppError::validation("Choose at least one warranty to claim."));
        }
        if body.items.len() > CLAIM_BATCH_MAX {
            return Err(AppError::validation(format!(
                "At most {CLAIM_BATCH_MAX} claims can be started at once."
            )));
        }
        let mut seen = HashSet::new();
        for item in &body.items {
            if !seen.insert(item.warranty_key.as_str()) {
                return Err(AppError::validation("A warranty can only be claimed once at a time."));
            }
        }

        let today = today();
        let mut invoice_key: Option<String> = None;
        let mut any_expired = false;
        for item in &body.items {
            let warranty = require(db, &item.warranty_key).await?;
            match &invoice_key {
                None => invoice_key = Some(warranty.invoice_key.clone()),
                Some(first) if *first != warranty.invoice_key => {
                    return Err(AppError::validation(
                        "These warranties are from different invoices. Claim them one invoice at a time.",
                    ));
                }
                Some(_) => {}
            }
            if warranty.into_warranty(today)?.state == WarrantyState::Expired {
                any_expired = true;
            }
        }

        let honoured_by = if any_expired && (body.approval.is_some() || caller.self_approves()) {
            Some(users::service::resolve_approval(db, caller, body.approval.as_ref()).await?)
        } else if body.approval.is_some() {
            return Err(AppError::validation(
                "No warranty here has run out, so no approval is needed.",
            ));
        } else {
            None
        };

        let mut results = Vec::with_capacity(body.items.len());
        for item in body.items {
            let warranty_key = item.warranty_key.clone();
            let request = OpenClaimRequest {
                warranty_key: item.warranty_key,
                issue_description: item.issue_description,
                void_checks: item.void_checks,
                conditions: item.conditions,
                quantity: item.quantity,
                serial_number: item.serial_number,
                assigned_employee_key: None,
                approval: None,
            };
            let opened =
                open_claim_honoured(db, request, honoured_by.as_ref(), actor, caller, device_id.clone())
                    .await;
            results.push(match opened {
                Ok(claim) => BatchClaimResult::Opened { warranty_key, claim: Box::new(claim) },
                Err(error) => BatchClaimResult::Failed { warranty_key, error: failure(&error) },
            });
        }

        let failed_count = results
            .iter()
            .filter(|result| matches!(result, BatchClaimResult::Failed { .. }))
            .count();
        Ok(OpenClaimsBatchResponse {
            opened_count: count(results.len() - failed_count),
            failed_count: count(failed_count),
            results,
        })
    })
    .await
}

fn count(value: usize) -> u32 {
    u32::try_from(value).expect("a batch has at most CLAIM_BATCH_MAX items")
}

/// What one item's own request would have answered with.
fn failure(error: &AppError) -> BatchClaimError {
    let parts = error.wire_parts();
    BatchClaimError {
        status: parts.status.as_u16(),
        code: parts.code,
        message: parts.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::constants::codes;

    #[test]
    fn failure_keeps_the_status_code_and_message_of_the_error() {
        let conflict = AppError::conflict(codes::CLAIM_ALREADY_OPEN, "Already open.");
        assert_eq!(
            failure(&conflict),
            BatchClaimError {
                status: 409,
                code: codes::CLAIM_ALREADY_OPEN.to_string(),
                message: "Already open.".to_string(),
            }
        );
        let invalid = failure(&AppError::validation("Write what is wrong."));
        assert_eq!(invalid.status, 400);
        assert_eq!(invalid.code, codes::VALIDATION_ERROR);
    }
}
