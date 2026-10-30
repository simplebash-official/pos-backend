use super::clean::clean;
use super::parse_day::parse_day;
use super::prepared_expense::PreparedExpense;
use super::reference_max::REFERENCE_MAX;
use super::text_max::TEXT_MAX;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::expenses::{CreateExpenseRequest, PaidFrom};
use crate::domain::sequences::ReserveSequenceRequest;
use crate::modules::expenses::model::{ExpenseDocument, ExpensePaymentDocument};
use crate::modules::{cash_categories, payees, sequences};
use mongodb::bson::DateTime as BsonDateTime;

const NAME_MAX: usize = 60;

/// Everything `create_expense` checks, with nothing written: the account, the amount, the dates,
/// the category, the payee, and the expense number taken.
pub(crate) async fn prepare_expense(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateExpenseRequest,
    device_id: Option<String>,
) -> AppResult<PreparedExpense> {
    {
        if body.paid_from == PaidFrom::Drawer {
            return Err(AppError::validation(
                "Cash paid from the drawer is recorded as a cash movement. It appears here by itself.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation(
                "Please enter an amount greater than zero.",
            ));
        }
        parse_day("expense date", &body.expense_date)?;
        let due_date = clean("The due date", body.due_date.as_deref(), 10)?;
        if let Some(due) = &due_date {
            if body.paid_from != PaidFrom::Unpaid {
                return Err(AppError::validation(
                    "A due date only applies to a bill that is not paid yet.",
                ));
            }
            parse_day("due date", due)?;
        }
        let category =
            cash_categories::service::category_for_expense(db, &body.category_key).await?;

        let typed = clean("The name", body.payee_name.as_deref(), NAME_MAX)?;
        let (payee_key, payee_name) = match (&body.payee_key, typed) {
            (Some(key), _) => {
                let resolved = payees::service::resolve_payee(db, key).await?;
                (Some(key.clone()), Some(resolved.name))
            }
            (None, Some(name)) => {
                let saved = payees::service::ensure_other_payee(db, &name).await?;
                (Some(saved.key), Some(saved.name))
            }
            (None, None) => (None, None),
        };

        let reservation = sequences::service::reserve_sequence(
            db,
            "expense".to_string(),
            ReserveSequenceRequest {
                block_size: Some(1),
                device_id,
            },
        )
        .await?;
        let expense_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let expense = ExpenseDocument {
            id: None,
            key: generate_id(prefixes::EXPENSE),
            expense_number,
            expense_date: body.expense_date.trim().to_string(),
            category_key: category.key,
            payee_key,
            payee_name,
            amount_cents: body.amount_cents,
            due_date,
            reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
            note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
            recorded_by_user_id: recorded_by_user_id.clone(),
            recorded_by_name_snapshot: recorded_by_name.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        };

        let payment = body.paid_from.is_account().then(|| ExpensePaymentDocument {
            id: None,
            key: generate_id(prefixes::EXPENSE_PAYMENT),
            expense_key: expense.key.clone(),
            amount_cents: expense.amount_cents,
            paid_from: body.paid_from,
            paid_on: expense.expense_date.clone(),
            reference_no: expense.reference_no.clone(),
            note: None,
            recorded_by_user_id,
            recorded_by_name_snapshot: recorded_by_name,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        });
        Ok(PreparedExpense { expense, payment })
    }
}
