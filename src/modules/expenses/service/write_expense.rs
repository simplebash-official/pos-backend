use super::prepared_expense::PreparedExpense;
use crate::clients::uow::Tx;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpenseDocument;
use crate::modules::expenses::repository;

/// Writes a prepared expense and its payment in the caller's transaction.
pub(crate) async fn write_expense(
    tx: &mut Tx,
    prepared: PreparedExpense,
) -> AppResult<ExpenseDocument> {
    let expense = repository::insert_expense(&mut *tx, prepared.expense).await?;
    if let Some(payment) = prepared.payment {
        repository::insert_payment(&mut *tx, payment).await?;
    }
    Ok(expense)
}
