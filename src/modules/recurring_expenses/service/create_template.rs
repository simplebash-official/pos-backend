use super::check_payee::check_payee;
use super::check_schedule::check_schedule;
use super::clean_note::clean_note;
use super::dtos::dtos;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::cash_categories::recurring_expense_key;
use crate::domain::recurring_expenses::{CreateRecurringExpenseRequest, RecurringExpense};
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use crate::modules::recurring_expenses::repository;
use crate::modules::{cash_categories, payees};
use mongodb::bson::DateTime as BsonDateTime;

/// Adds a regular bill. The category must count as a business cost; one bill per category and payee.
pub async fn create_template(
    db: &Db,
    body: CreateRecurringExpenseRequest,
) -> AppResult<RecurringExpense> {
    tracked("recurring_expenses.created", async move {
        check_schedule(
            body.amount_cents,
            body.due_day,
            body.remind_days_before,
            body.paid_from,
        )?;
        let category =
            cash_categories::service::category_for_expense(db, &body.category_key).await?;
        check_payee(
            category.movement_type,
            body.paid_from,
            body.payee_key.as_deref(),
        )?;
        if let Some(payee_key) = &body.payee_key {
            payees::service::resolve_payee(db, payee_key).await?;
        }
        let key = recurring_expense_key(&category.key, body.payee_key.as_deref());
        if repository::find_by_key(db, &key).await?.is_some() {
            return Err(AppError::conflict(
                codes::RECURRING_EXPENSE_EXISTS,
                "There is already a regular bill for this. Change that one instead.",
            ));
        }
        let now = BsonDateTime::now();
        let inserted = repository::insert(
            db,
            RecurringExpenseDocument {
                id: None,
                key,
                category_key: category.key,
                payee_key: body.payee_key,
                amount_cents: body.amount_cents,
                due_day: body.due_day,
                remind_days_before: body.remind_days_before,
                paid_from: body.paid_from,
                note: clean_note(body.note.as_deref())?,
                active: true,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(dtos(db, std::slice::from_ref(&inserted)).await?.remove(0))
    })
    .await
}
