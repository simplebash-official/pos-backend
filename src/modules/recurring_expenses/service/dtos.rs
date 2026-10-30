use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::cash_categories::PayeeListQuery;
use crate::domain::recurring_expenses::RecurringExpense;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use crate::modules::{cash_categories, payees};
use std::collections::HashMap;

/// Names for every payee including hidden ones: a bill keeps working after its payee is hidden.
async fn payee_names(db: &Db) -> AppResult<HashMap<String, String>> {
    Ok(payees::service::list_payees(
        db,
        PayeeListQuery {
            include_hidden: Some(true),
            ..Default::default()
        },
    )
    .await?
    .into_iter()
    .map(|p| (p.key, p.name))
    .collect())
}

fn to_dto(
    doc: &RecurringExpenseDocument,
    category_name: String,
    payee_name: Option<String>,
) -> RecurringExpense {
    RecurringExpense {
        id: doc
            .id
            .map(|o| o.to_hex())
            .unwrap_or_else(|| doc.key.clone()),
        key: doc.key.clone(),
        category_key: doc.category_key.clone(),
        category_name,
        payee_key: doc.payee_key.clone(),
        payee_name,
        amount_cents: doc.amount_cents,
        due_day: doc.due_day,
        remind_days_before: doc.remind_days_before,
        paid_from: doc.paid_from,
        note: doc.note.clone(),
        active: doc.active,
        version: doc.version,
        created_at: doc.created_at.to_chrono(),
        updated_at: doc.updated_at.to_chrono(),
    }
}

pub(super) async fn dtos(
    db: &Db,
    docs: &[RecurringExpenseDocument],
) -> AppResult<Vec<RecurringExpense>> {
    let mut keys: Vec<String> = docs.iter().map(|d| d.category_key.clone()).collect();
    keys.sort_unstable();
    keys.dedup();
    let categories = cash_categories::service::display_names(db, &keys).await?;
    let names = payee_names(db).await?;
    Ok(docs
        .iter()
        .map(|d| {
            to_dto(
                d,
                categories[&d.category_key].clone(),
                d.payee_key.as_ref().and_then(|k| names.get(k).cloned()),
            )
        })
        .collect())
}
