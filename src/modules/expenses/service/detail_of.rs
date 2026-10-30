use super::ledger_row::ledger_row;
use super::today::today;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::expenses::{ExpenseDetail, ExpensePayment};
use crate::modules::cash_categories;
use crate::modules::expenses::model::{ExpenseDocument, ExpensePaymentDocument};
use crate::modules::expenses::repository;

fn payment_dto(p: &ExpensePaymentDocument) -> ExpensePayment {
    ExpensePayment {
        id: p.id.map(|o| o.to_hex()).unwrap_or_else(|| p.key.clone()),
        key: p.key.clone(),
        expense_key: p.expense_key.clone(),
        amount_cents: p.amount_cents,
        paid_from: p.paid_from,
        paid_on: p.paid_on.clone(),
        reference_no: p.reference_no.clone(),
        note: p.note.clone(),
        recorded_by_name_snapshot: p.recorded_by_name_snapshot.clone(),
        created_at: p.created_at.to_chrono(),
    }
}

pub(super) async fn detail_of(db: &Db, expense: &ExpenseDocument) -> AppResult<ExpenseDetail> {
    let payments = repository::list_payments_for(db, &expense.key).await?;
    let categories = cash_categories::service::resolve_categories(
        db,
        std::slice::from_ref(&expense.category_key),
    )
    .await?;
    let refs: Vec<&ExpensePaymentDocument> = payments.iter().collect();
    Ok(ExpenseDetail {
        expense: ledger_row(expense, &refs, &categories[&expense.category_key], &today()),
        payments: payments.iter().map(payment_dto).collect(),
    })
}
