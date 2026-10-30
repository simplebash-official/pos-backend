use super::detail_of::detail_of;
use super::prepare_expense::prepare_expense;
use super::write_expense::write_expense;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::domain::tracked;
use crate::domain::expenses::{CreateExpenseRequest, ExpenseDetail};
use crate::domain::purchase_receipts::PaymentKind;
use crate::modules::purchases::service::settlement;

/// Enters a cost paid from the bank, a card or online (paid in full on its date), or a bill still
/// to pay. Cash from the drawer is a cash movement, not an entry here.
pub async fn create_expense(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateExpenseRequest,
    device_id: Option<String>,
) -> AppResult<ExpenseDetail> {
    tracked("expenses.created", async move {
        let requested = body.settlements.clone();
        let prepared =
            prepare_expense(db, recorded_by_user_id, recorded_by_name, body, device_id).await?;
        let settlements = if requested.is_empty() {
            None
        } else {
            let payment_key = prepared.payment_key().ok_or_else(|| {
                AppError::validation(
                    "Receipts can only be paid by a payment, not a bill still to pay.",
                )
            })?;
            let supplier_key = prepared.expense.payee_key.as_deref().ok_or_else(|| {
                AppError::validation("Pick the supplier whose receipts are being paid.")
            })?;
            if !supplier_key.starts_with(prefixes::SUPPLIER) {
                return Err(AppError::validation(
                    "Only a supplier's receipts can be paid.",
                ));
            }
            Some((
                settlement::prepare_settlements(
                    db,
                    supplier_key,
                    prepared.expense.amount_cents,
                    &requested,
                )
                .await?,
                payment_key.to_string(),
            ))
        };
        let mut tx = db.begin().await?;
        let expense = write_expense(&mut tx, prepared).await?;
        if let Some((prepared_settlements, payment_key)) = settlements {
            settlement::write_settlements(
                &mut tx,
                prepared_settlements,
                PaymentKind::ExpensePayment,
                &payment_key,
            )
            .await?;
        }
        tx.commit().await?;
        detail_of(db, &expense).await
    })
    .await
}
