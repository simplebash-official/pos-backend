use super::owed::owed;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::balances::{BalanceSubject, SupplierBalance, SupplierBalancesResponse};
use crate::domain::sessions::CashMovementType;
use crate::domain::suppliers::SupplierListQuery;
use crate::modules::balances::model::BalanceOpeningDocument;
use crate::modules::balances::repository;
use crate::modules::{expenses, purchases, sessions, suppliers};
use std::collections::HashMap;

/// Every supplier with what the shop owes them. A supplier has no balance until an opening balance is set.
pub async fn list_supplier_balances(db: &Db) -> AppResult<SupplierBalancesResponse> {
    let openings: HashMap<String, BalanceOpeningDocument> =
        repository::list_openings(db, BalanceSubject::Supplier)
            .await?
            .into_iter()
            .map(|o| (o.subject_key.clone(), o))
            .collect();
    let mut payouts: Vec<(String, String, i64)> = Vec::new();
    for p in sessions::service::payouts_of_type(db, CashMovementType::SupplierPayment).await? {
        if let Some(payee) = p.payee_key {
            payouts.push((payee, p.day, p.amount_cents));
        }
    }
    for p in expenses::service::ledger_payouts(db).await? {
        if let Some(payee) = p.payee_key {
            payouts.push((payee, p.day, p.amount_cents));
        }
    }

    let mut out = Vec::new();
    for s in suppliers::service::list_suppliers(db, SupplierListQuery::default())
        .await?
        .suppliers
    {
        let mut balance = SupplierBalance {
            supplier_key: s.key.clone(),
            name: s.name,
            opening_balance_cents: None,
            as_of_date: None,
            purchased_cents: None,
            paid_cents: None,
            credited_cents: None,
            balance_cents: None,
        };
        if let Some(o) = openings.get(&s.key) {
            // shortcut: one purchases query per supplier with an opening; upgrade to a grouped query for hundreds of suppliers.
            let purchased: i64 = purchases::service::purchase::costs_of_supplier(db, &s.key)
                .await?
                .into_iter()
                .filter(|(day, _)| day.as_str() >= o.as_of_date.as_str())
                .map(|(_, cents)| cents)
                .sum();
            let paid: i64 = payouts
                .iter()
                .filter(|(payee, day, _)| *payee == s.key && day.as_str() >= o.as_of_date.as_str())
                .map(|(_, _, cents)| cents)
                .sum();
            let credited: i64 = purchases::service::credit::credits_of_supplier(db, &s.key)
                .await?
                .into_iter()
                .filter(|(day, _)| day.as_str() >= o.as_of_date.as_str())
                .map(|(_, cents)| cents)
                .sum();
            balance.opening_balance_cents = Some(o.opening_balance_cents);
            balance.as_of_date = Some(o.as_of_date.clone());
            balance.purchased_cents = Some(purchased);
            balance.paid_cents = Some(paid);
            balance.credited_cents = Some(credited);
            balance.balance_cents = Some(owed(o.opening_balance_cents, purchased, paid + credited));
        }
        out.push(balance);
    }
    // Biggest debt first; suppliers not set up yet last.
    out.sort_by(|a, b| {
        b.balance_cents
            .cmp(&a.balance_cents)
            .then(a.name.cmp(&b.name))
    });
    Ok(SupplierBalancesResponse { suppliers: out })
}
