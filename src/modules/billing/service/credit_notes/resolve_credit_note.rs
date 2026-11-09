use super::create_credit_note::{
    apply_return_stock_movement, transition_returned_serial, validate_condition_disposition,
};
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::billing::{
    CreditNote, CreditNoteResolution, CreditNoteStatus, ItemCondition, ResolveCreditNoteRequest,
};
use crate::domain::users::ApprovalCaller;
use crate::modules::billing::repository;
use crate::modules::users::service::resolve_approval;
use chrono::Utc;
use std::collections::BTreeSet;

/// Decides what happens to every unit a credit note holds for inspection: restocks it, writes
/// it off, returns it to the supplier or parks it for repair, per the same mapping creation
/// uses, then moves the note to `Resolved`. Every pending line must be decided in one request.
pub async fn resolve_credit_note(
    db: &Db,
    id_or_key: &str,
    caller: &ApprovalCaller,
    body: ResolveCreditNoteRequest,
) -> AppResult<CreditNote> {
    crate::core::logging::domain::tracked("billing.credit_note_resolved", async move {
        let existing = repository::credit_notes::find_credit_note_by_id_or_key(db, id_or_key)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Credit note not found", codes::CREDIT_NOTE_NOT_FOUND)
            })?;
        if existing.status != CreditNoteStatus::AwaitingResolution {
            return Err(AppError::conflict(
                codes::CREDIT_NOTE_NOT_AWAITING_RESOLUTION,
                "This credit note is not waiting for inspection",
            ));
        }

        let pending: BTreeSet<usize> = existing
            .returned_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.condition == ItemCondition::PendingInspection)
            .map(|(index, _)| index)
            .collect();
        let decided: BTreeSet<usize> = body.lines.iter().map(|line| line.line_index).collect();
        if decided.len() != body.lines.len() || decided != pending {
            return Err(AppError::validation(
                "Decide every line held for inspection, each exactly once",
            ));
        }
        for line in &body.lines {
            if line.condition == ItemCondition::PendingInspection {
                return Err(AppError::validation(
                    "A line must be resolved to a real condition, not 'pending_inspection'",
                ));
            }
            validate_condition_disposition(line.condition, line.disposition)?;
        }

        let approved = resolve_approval(db, caller, body.approval.as_ref()).await?;
        let resolution = CreditNoteResolution {
            resolved_at: Utc::now(),
            resolved_by_user_id: caller.user_id.clone(),
            resolved_by_name: caller.name.clone(),
            approved_by_user_id: approved.user_id,
            approved_by_name: approved.name,
            lines: body.lines,
        };

        let object_id = existing
            .id
            .expect("persisted credit note document must have an _id");
        // The guarded write comes first: whoever wins it applies the stock and serial effects
        // once, and a repeat or a lost race stops here.
        let updated = repository::credit_notes::resolve_credit_note(
            db,
            object_id,
            existing.version,
            &resolution,
        )
        .await?
        .ok_or_else(|| {
            AppError::conflict(
                codes::CREDIT_NOTE_RESOLVE_CONFLICT,
                "This credit note changed while you were resolving it. Reload and try again",
            )
        })?;

        for line in &resolution.lines {
            let mut item = existing.returned_items[line.line_index].clone();
            item.condition = line.condition;
            item.disposition = line.disposition;
            apply_return_stock_movement(db, &item, &existing.key, &existing.credit_note_number)
                .await;
            transition_returned_serial(db, &item, &existing.key).await;
        }

        Ok(updated.into_credit_note())
    })
    .await
}
