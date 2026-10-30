use super::check_payee::check_payee;
use super::check_schedule::check_schedule;
use super::clean_note::clean_note;
use super::dtos::dtos;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::recurring_expenses::{RecurringExpense, UpdateRecurringExpenseRequest};
use crate::modules::cash_categories;
use crate::modules::recurring_expenses::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Changes the amount, schedule, usual account or note of a regular bill, or stops or restarts it.
pub async fn update_template(
    db: &Db,
    key: &str,
    body: UpdateRecurringExpenseRequest,
) -> AppResult<RecurringExpense> {
    tracked("recurring_expenses.updated", async move {
        let mut doc = repository::find_by_key(db, key).await?.ok_or_else(|| {
            AppError::not_found_with_code(
                "Regular bill not found.",
                codes::RECURRING_EXPENSE_NOT_FOUND,
            )
        })?;
        doc.amount_cents = body.amount_cents.unwrap_or(doc.amount_cents);
        doc.due_day = body.due_day.unwrap_or(doc.due_day);
        doc.remind_days_before = body.remind_days_before.unwrap_or(doc.remind_days_before);
        doc.paid_from = body.paid_from.unwrap_or(doc.paid_from);
        check_schedule(
            doc.amount_cents,
            doc.due_day,
            doc.remind_days_before,
            doc.paid_from,
        )?;
        // Only a change of account is checked, so a bill can always be stopped.
        if body.paid_from.is_some() {
            let category =
                cash_categories::service::category_for_expense(db, &doc.category_key).await?;
            check_payee(
                category.movement_type,
                doc.paid_from,
                doc.payee_key.as_deref(),
            )?;
        }
        if let Some(note) = &body.note {
            doc.note = clean_note(Some(note))?;
        }
        if let Some(active) = body.active {
            doc.active = active;
        }
        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        Ok(dtos(db, std::slice::from_ref(&doc)).await?.remove(0))
    })
    .await
}
