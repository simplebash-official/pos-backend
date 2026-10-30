use super::clean::clean;
use super::detail_of::detail_of;
use super::parse_day::parse_day;
use super::reference_max::REFERENCE_MAX;
use super::text_max::TEXT_MAX;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::logging::domain::tracked;
use crate::domain::expenses::{CreateExpensePaymentRequest, ExpenseDetail, PaidFrom};
use crate::modules::expenses::model::ExpensePaymentDocument;
use crate::modules::expenses::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Records a payment against a bill, from the bank, a card or online. Never more than is still owed.
pub async fn add_payment(
    db: &Db,
    expense_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateExpensePaymentRequest,
) -> AppResult<ExpenseDetail> {
    tracked("expenses.payment_recorded", async move {
        if !body.paid_from.is_account() || body.paid_from == PaidFrom::Drawer {
            return Err(AppError::validation(
                "Pick the bank, a card or online. Cash from the drawer is recorded as a cash movement.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation("Please enter an amount greater than zero."));
        }
        parse_day("payment date", &body.paid_on)?;
        let expense = repository::find_expense_by_key(db, expense_key).await?.ok_or_else(|| {
            AppError::not_found_with_code("Expense not found.", codes::EXPENSE_NOT_FOUND)
        })?;
        let paid: i64 = repository::list_payments_for(db, expense_key)
            .await?
            .iter()
            .map(|p| p.amount_cents)
            .sum();
        if body.amount_cents > expense.amount_cents - paid {
            return Err(AppError::conflict(
                codes::EXPENSE_OVERPAID,
                "This is more than is still owed on the bill.",
            ));
        }
        let now = BsonDateTime::now();
        repository::insert_payment(
            db,
            ExpensePaymentDocument {
                id: None,
                key: generate_id(prefixes::EXPENSE_PAYMENT),
                expense_key: expense.key.clone(),
                amount_cents: body.amount_cents,
                paid_from: body.paid_from,
                paid_on: body.paid_on.trim().to_string(),
                reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
                note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        detail_of(db, &expense).await
    })
    .await
}
