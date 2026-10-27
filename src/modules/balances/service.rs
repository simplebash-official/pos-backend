// Business rules for supplier balances and staff advances: where each starts, what has moved since,
// and recording the part of an advance taken back from pay. Nothing here is a stored balance.

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        logging::domain::tracked,
    },
    domain::{
        balances::{
            AdvanceGiven, AdvanceRecovery, BalanceSubject, CreateAdvanceRecoveryRequest,
            SetBalanceOpeningRequest, StaffAdvance, StaffAdvanceDetail, StaffAdvancesResponse,
            SupplierBalance, SupplierBalancesResponse,
        },
        cash_categories::category_key,
        employees::EmployeeListQuery,
        sessions::CashMovementType,
        suppliers::SupplierListQuery,
    },
    modules::{
        balances::{
            model::{AdvanceRecoveryDocument, BalanceOpeningDocument},
            repository,
        },
        cash_categories, employees, expenses, purchases, sessions, suppliers,
    },
};

/// The staff-payment categories that are money the staff member owes back.
const ADVANCE_CATEGORIES: [&str; 2] = ["Advance", "Staff loan"];
const NOTE_MAX: usize = 200;

fn opening_key(subject_key: &str) -> String {
    format!("{}_{subject_key}", prefixes::BALANCE_OPENING)
}

/// A balance: where it started, plus what increased it, less what reduced it.
fn owed(opening: i64, increases: i64, reductions: i64) -> i64 {
    opening + increases - reductions
}

fn parse_day(value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Please pick a valid date."))
}

/// One advance handed out, from the drawer or the ledger.
struct Given {
    employee_key: String,
    category_key: String,
    day: String,
    number: String,
    cents: i64,
}

/// Every advance or staff loan handed out, to anyone with a staff key.
async fn advances_given(db: &Db) -> AppResult<Vec<Given>> {
    let mut keys = Vec::new();
    for name in ADVANCE_CATEGORIES {
        keys.extend(
            cash_categories::service::keys_including_merged(
                db,
                &category_key(CashMovementType::StaffPayment, name),
            )
            .await?,
        );
    }
    let is_advance = |category: &str| keys.iter().any(|k| k == category);
    let is_staff = |payee: &Option<String>| payee.as_deref().is_some_and(|k| k.starts_with("emp_"));
    let mut out: Vec<Given> =
        sessions::service::payouts_of_type(db, CashMovementType::StaffPayment)
            .await?
            .into_iter()
            .filter(|p| is_advance(&p.category_key) && is_staff(&p.payee_key))
            .map(|p| Given {
                employee_key: p.payee_key.expect("filtered to staff"),
                category_key: p.category_key,
                day: p.day,
                number: p.number,
                cents: p.amount_cents,
            })
            .collect();
    out.extend(
        expenses::service::ledger_payouts(db)
            .await?
            .into_iter()
            .filter(|p| is_advance(&p.category_key) && is_staff(&p.payee_key))
            .map(|p| Given {
                employee_key: p.payee_key.expect("filtered to staff"),
                category_key: p.category_key,
                day: p.day,
                number: p.number,
                cents: p.amount_cents,
            }),
    );
    Ok(out)
}

fn recovery_dto(r: &AdvanceRecoveryDocument) -> AdvanceRecovery {
    AdvanceRecovery {
        key: r.key.clone(),
        employee_key: r.employee_key.clone(),
        amount_cents: r.amount_cents,
        recovered_on: r.recovered_on.clone(),
        note: r.note.clone(),
        recorded_by_name_snapshot: r.recorded_by_name_snapshot.clone(),
        created_at: r.created_at.to_chrono(),
    }
}

/// What one staff member owes: the opening (if any), plus advances given, less advances recovered, both from the opening day.
fn advance_summary(
    employee_key: &str,
    name: String,
    opening: Option<&BalanceOpeningDocument>,
    given: &[&Given],
    recoveries: &[&AdvanceRecoveryDocument],
) -> StaffAdvance {
    let since = opening.map(|o| o.as_of_date.as_str());
    let counts = |day: &str| since.is_none_or(|s| day >= s);
    let given_cents: i64 = given
        .iter()
        .filter(|g| counts(&g.day))
        .map(|g| g.cents)
        .sum();
    let recovered_cents: i64 = recoveries
        .iter()
        .filter(|r| counts(&r.recovered_on))
        .map(|r| r.amount_cents)
        .sum();
    StaffAdvance {
        employee_key: employee_key.to_string(),
        name,
        opening_balance_cents: opening.map(|o| o.opening_balance_cents),
        as_of_date: opening.map(|o| o.as_of_date.clone()),
        given_cents,
        recovered_cents,
        outstanding_cents: owed(
            opening.map_or(0, |o| o.opening_balance_cents),
            given_cents,
            recovered_cents,
        ),
        last_given_on: given.iter().map(|g| g.day.clone()).max(),
    }
}

/// Everyone who has been given an advance (or has an opening or a recovery), with what they still owe.
pub async fn list_staff_advances(db: &Db) -> AppResult<StaffAdvancesResponse> {
    let given = advances_given(db).await?;
    let recoveries = repository::list_recoveries(db).await?;
    let openings: HashMap<String, BalanceOpeningDocument> =
        repository::list_openings(db, BalanceSubject::Staff)
            .await?
            .into_iter()
            .map(|o| (o.subject_key.clone(), o))
            .collect();
    let mut staff = Vec::new();
    for e in employees::service::list_employees(db, EmployeeListQuery::default())
        .await?
        .employees
    {
        let g: Vec<&Given> = given.iter().filter(|g| g.employee_key == e.key).collect();
        let r: Vec<&AdvanceRecoveryDocument> = recoveries
            .iter()
            .filter(|r| r.employee_key == e.key)
            .collect();
        let opening = openings.get(&e.key);
        if g.is_empty() && r.is_empty() && opening.is_none() {
            continue;
        }
        staff.push(advance_summary(&e.key, e.name, opening, &g, &r));
    }
    staff.sort_by(|a, b| {
        b.outstanding_cents
            .cmp(&a.outstanding_cents)
            .then(a.name.cmp(&b.name))
    });
    Ok(StaffAdvancesResponse { staff })
}

/// One staff member's advances handed out and recovered.
pub async fn get_staff_advance(db: &Db, employee_key: &str) -> AppResult<StaffAdvanceDetail> {
    let employee = employees::service::get_employee_by_key(db, employee_key).await?;
    let all_given = advances_given(db).await?;
    let given: Vec<&Given> = all_given
        .iter()
        .filter(|g| g.employee_key == employee_key)
        .collect();
    let all_recoveries = repository::list_recoveries(db).await?;
    let recoveries: Vec<&AdvanceRecoveryDocument> = all_recoveries
        .iter()
        .filter(|r| r.employee_key == employee_key)
        .collect();
    let opening = repository::find_opening(db, &opening_key(employee_key)).await?;
    let summary = advance_summary(
        employee_key,
        employee.name,
        opening.as_ref(),
        &given,
        &recoveries,
    );

    let since = summary.as_of_date.clone();
    let since = since.as_deref();
    let names = cash_categories::service::display_names(
        db,
        &given
            .iter()
            .map(|g| g.category_key.clone())
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut entries: Vec<&Given> = given
        .iter()
        .copied()
        .filter(|g| since.is_none_or(|s| g.day.as_str() >= s))
        .collect();
    entries.sort_by(|a, b| b.day.cmp(&a.day).then(b.number.cmp(&a.number)));
    Ok(StaffAdvanceDetail {
        summary,
        given: entries
            .into_iter()
            .map(|g| AdvanceGiven {
                number: g.number.clone(),
                date: g.day.clone(),
                amount_cents: g.cents,
                category_name: names[&g.category_key].clone(),
            })
            .collect(),
        recoveries: recoveries
            .into_iter()
            .filter(|r| since.is_none_or(|s| r.recovered_on.as_str() >= s))
            .rev()
            .map(recovery_dto)
            .collect(),
    })
}

/// Records part of an advance taken back from pay; never more than the staff member still owes.
pub async fn recover_advance(
    db: &Db,
    employee_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateAdvanceRecoveryRequest,
) -> AppResult<StaffAdvanceDetail> {
    tracked("balances.advance_recovered", async move {
        if body.amount_cents <= 0 {
            return Err(AppError::validation(
                "Please enter an amount greater than zero.",
            ));
        }
        parse_day(&body.recovered_on)?;
        let note = body
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        if note.is_some_and(|n| n.chars().count() > NOTE_MAX) {
            return Err(AppError::validation(format!(
                "The note can have up to {NOTE_MAX} letters."
            )));
        }
        let outstanding = get_staff_advance(db, employee_key)
            .await?
            .summary
            .outstanding_cents;
        if body.amount_cents > outstanding {
            return Err(AppError::conflict(
                codes::ADVANCE_OVER_RECOVERED,
                "This is more than the staff member still owes on advances.",
            ));
        }
        let now = BsonDateTime::now();
        repository::insert_recovery(
            db,
            AdvanceRecoveryDocument {
                id: None,
                key: generate_id(prefixes::ADVANCE_RECOVERY),
                employee_key: employee_key.to_string(),
                amount_cents: body.amount_cents,
                recovered_on: body.recovered_on.trim().to_string(),
                note: note.map(str::to_string),
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        get_staff_advance(db, employee_key).await
    })
    .await
}

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

/// Sets, or changes, where a supplier balance (or a staff member's earlier advance) starts.
pub async fn set_opening(
    db: &Db,
    subject: BalanceSubject,
    subject_key: &str,
    set_by_name: String,
    body: SetBalanceOpeningRequest,
) -> AppResult<()> {
    tracked("balances.opening_set", async move {
        match subject {
            BalanceSubject::Supplier => {
                suppliers::service::get_supplier_by_key(db, subject_key).await?;
            }
            BalanceSubject::Staff => {
                employees::service::get_employee_by_key(db, subject_key).await?;
            }
        }
        let as_of = parse_day(&body.as_of_date)?;
        if as_of > Utc::now().date_naive() {
            return Err(AppError::validation("The date cannot be in the future."));
        }
        let as_of_date = as_of.format("%Y-%m-%d").to_string();
        let key = opening_key(subject_key);
        let now = BsonDateTime::now();
        match repository::find_opening(db, &key).await? {
            Some(mut existing) => {
                existing.opening_balance_cents = body.opening_balance_cents;
                existing.as_of_date = as_of_date;
                existing.set_by_name_snapshot = set_by_name;
                existing.version += 1;
                existing.updated_at = now;
                repository::save_opening(db, &existing).await?;
            }
            None => {
                repository::insert_opening(
                    db,
                    BalanceOpeningDocument {
                        id: None,
                        key,
                        subject_kind: subject,
                        subject_key: subject_key.to_string(),
                        opening_balance_cents: body.opening_balance_cents,
                        as_of_date,
                        set_by_name_snapshot: set_by_name,
                        version: 1,
                        created_at: now,
                        updated_at: now,
                        updated_by_device: None,
                    },
                )
                .await?;
            }
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balance_is_the_opening_plus_increases_less_reductions() {
        assert_eq!(owed(1000, 500, 300), 1200);
        assert_eq!(owed(0, 0, 250), -250, "paid ahead is a negative balance");
    }

    #[test]
    fn opening_keys_follow_the_subject() {
        assert_eq!(opening_key("sup_abc"), "bopen_sup_abc");
    }
}
