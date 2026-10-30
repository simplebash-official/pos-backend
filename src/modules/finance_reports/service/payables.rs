use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::finance_reports::{PayableBill, PayableSupplier, PayablesResponse};
use crate::domain::purchase_receipts::ReceiptListQuery;
use crate::modules::{balances, expenses, purchases};
use chrono::Utc;
use std::collections::HashMap;

/// What the shop still has to pay today: bills not paid in full, and suppliers it owes. A supplier with an
/// opening balance is listed by its balance. One without is listed by what its supplier receipts still owe,
/// so a supplier is never counted both ways.
pub async fn payables(db: &Db) -> AppResult<PayablesResponse> {
    let mut bills: Vec<PayableBill> = expenses::service::unpaid_rows(db)
        .await?
        .into_iter()
        .map(|r| PayableBill {
            key: r.key,
            number: r.number,
            category_name: r.category_name,
            payee_name: r.payee_name,
            amount_cents: r.amount_cents,
            balance_cents: r.balance_cents,
            due_date: r.due_date,
            is_overdue: r.is_overdue,
        })
        .collect();
    // Soonest due first; a bill with no due date waits at the end.
    bills.sort_by(|a, b| match (&a.due_date, &b.due_date) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.number.cmp(&b.number),
    });
    let bills_cents = bills.iter().map(|b| b.balance_cents).sum::<i64>();
    let overdue_cents = bills
        .iter()
        .filter(|b| b.is_overdue)
        .map(|b| b.balance_cents)
        .sum::<i64>();

    let mut suppliers: Vec<PayableSupplier> = balances::service::list_supplier_balances(db)
        .await?
        .suppliers
        .into_iter()
        .filter_map(|s| match s.balance_cents {
            Some(balance) if balance > 0 => Some(PayableSupplier {
                supplier_key: s.supplier_key,
                name: s.name,
                balance_cents: balance,
            }),
            _ => None,
        })
        .collect();
    // Suppliers with no opening balance: what their open receipts still owe.
    let without_opening: HashMap<String, String> = balances::service::list_supplier_balances(db)
        .await?
        .suppliers
        .into_iter()
        .filter(|s| s.balance_cents.is_none())
        .map(|s| (s.supplier_key, s.name))
        .collect();
    let open_receipts = purchases::service::receipt::list_receipts(
        db,
        ReceiptListQuery {
            open: Some(true),
            limit: Some(500),
            ..Default::default()
        },
    )
    .await?
    .items;
    let mut due_by_supplier: HashMap<String, i64> = HashMap::new();
    for receipt in open_receipts {
        if without_opening.contains_key(&receipt.supplier_key) {
            *due_by_supplier.entry(receipt.supplier_key).or_insert(0) += receipt.due_cents;
        }
    }
    for (supplier_key, balance_cents) in due_by_supplier {
        let name = without_opening[&supplier_key].clone();
        suppliers.push(PayableSupplier {
            supplier_key,
            name,
            balance_cents,
        });
    }
    suppliers.sort_by(|a, b| {
        b.balance_cents
            .cmp(&a.balance_cents)
            .then(a.name.cmp(&b.name))
    });
    let suppliers_cents = suppliers.iter().map(|s| s.balance_cents).sum::<i64>();

    Ok(PayablesResponse {
        as_of: Utc::now().date_naive().format("%Y-%m-%d").to_string(),
        bills,
        bills_cents,
        overdue_cents,
        suppliers,
        suppliers_cents,
        total_cents: bills_cents + suppliers_cents,
    })
}
