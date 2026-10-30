use super::get_staff_advance::get_staff_advance;
use super::parse_day::parse_day;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::logging::domain::tracked;
use crate::domain::balances::{CreateAdvanceRecoveryRequest, StaffAdvanceDetail};
use crate::modules::balances::model::AdvanceRecoveryDocument;
use crate::modules::balances::repository;
use mongodb::bson::DateTime as BsonDateTime;

const NOTE_MAX: usize = 200;

/// Records part of an advance taken back from pay; never more than the staff member still owes.
pub async fn recover_advance(
    db: &Db,
    employee_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateAdvanceRecoveryRequest,
) -> AppResult<StaffAdvanceDetail> {
    tracked("balances.advance_recovered", async move {
        if body.amount_cents <= 0 {
            return Err(AppError::validation(
                "Please enter an amount greater than zero.",
            ));
        }
        parse_day(&body.recovered_on)?;
        let note = body
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        if note.is_some_and(|n| n.chars().count() > NOTE_MAX) {
            return Err(AppError::validation(format!(
                "The note can have up to {NOTE_MAX} letters."
            )));
        }
        let outstanding = get_staff_advance(db, employee_key)
            .await?
            .summary
            .outstanding_cents;
        if body.amount_cents > outstanding {
            return Err(AppError::conflict(
                codes::ADVANCE_OVER_RECOVERED,
                "This is more than the staff member still owes on advances.",
            ));
        }
        let now = BsonDateTime::now();
        repository::insert_recovery(
            db,
            AdvanceRecoveryDocument {
                id: None,
                key: generate_id(prefixes::ADVANCE_RECOVERY),
                employee_key: employee_key.to_string(),
                amount_cents: body.amount_cents,
                recovered_on: body.recovered_on.trim().to_string(),
                note: note.map(str::to_string),
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        get_staff_advance(db, employee_key).await
    })
    .await
}
