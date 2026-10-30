use super::ledger_payout::LedgerPayout;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpenseDocument;
use crate::modules::expenses::repository;
use std::collections::HashMap;

/// Every payment made against a ledger expense (bank, card or online), with who it was for and what for.
pub(crate) async fn ledger_payouts(db: &Db) -> AppResult<Vec<LedgerPayout>> {
    let expenses = repository::list_expenses(db, None, None).await?;
    let by_key: HashMap<&str, &ExpenseDocument> =
        expenses.iter().map(|e| (e.key.as_str(), e)).collect();
    Ok(repository::list_all_payments(db)
        .await?
        .iter()
        .filter_map(|p| {
            let e = by_key.get(p.expense_key.as_str())?;
            Some(LedgerPayout {
                payee_key: e.payee_key.clone(),
                category_key: e.category_key.clone(),
                day: p.paid_on.clone(),
                number: e.expense_number.clone(),
                amount_cents: p.amount_cents,
            })
        })
        .collect())
}
