// Business rules for regular bills: what a template may say, how a bill stands for a month, and
// the "already paid" check used before a rent payment is recorded.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::codes,
        error::{AppError, AppResult},
        logging::domain::tracked,
    },
    domain::{
        cash_categories::{PayeeListQuery, recurring_expense_key},
        sessions::CashMovementType,
        expenses::PaidFrom,
        recurring_expenses::{
            BillStatus, CreateRecurringExpenseRequest, PeriodStatus, PeriodStatusQuery, RecurringBill,
            RecurringExpense, RecurringExpenseListQuery, UpdateRecurringExpenseRequest,
        },
    },
    modules::{
        cash_categories, expenses, payees,
        recurring_expenses::{model::RecurringExpenseDocument, repository},
    },
};

const REMIND_MAX_DAYS: i64 = 30;
const NOTE_MAX: usize = 200;

fn clean_note(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > NOTE_MAX {
        return Err(AppError::validation(format!("The note can have up to {NOTE_MAX} letters.")));
    }
    Ok(Some(text.to_string()))
}

fn check_schedule(amount_cents: i64, due_day: i64, remind_days_before: i64, paid_from: PaidFrom) -> AppResult<()> {
    if amount_cents <= 0 {
        return Err(AppError::validation("Please enter an amount greater than zero."));
    }
    if !(1..=31).contains(&due_day) {
        return Err(AppError::validation("The due day must be between 1 and 31."));
    }
    if !(0..=REMIND_MAX_DAYS).contains(&remind_days_before) {
        return Err(AppError::validation(format!(
            "Reminders can start 0 to {REMIND_MAX_DAYS} days before the due day."
        )));
    }
    if paid_from == PaidFrom::Unpaid {
        return Err(AppError::validation("Pick where it is usually paid from."));
    }
    Ok(())
}

/// Rent and bills paid from the drawer are always paid to someone (the cash form asks who), so a
/// regular bill that will be paid that way must name them or it could never be paid.
fn check_payee(
    movement_type: CashMovementType,
    paid_from: PaidFrom,
    payee_key: Option<&str>,
) -> AppResult<()> {
    if movement_type == CashMovementType::RentAndBills
        && paid_from == PaidFrom::Drawer
        && payee_key.is_none()
    {
        return Err(AppError::validation("Pick who is being paid."));
    }
    Ok(())
}

/// A month (`YYYY-MM`) as its first day.
fn parse_month(period: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Please pick a valid month."))
}

fn month_string(first_day: NaiveDate) -> String {
    first_day.format("%Y-%m").to_string()
}

fn previous_month(first_day: NaiveDate) -> NaiveDate {
    first_day.pred_opt().expect("a day before exists").with_day(1).expect("day 1 exists")
}

/// The day a bill falls due in a month: the due day, or the month's last day when it has fewer days.
fn due_date_in(first_day: NaiveDate, due_day: i64) -> NaiveDate {
    let next_month = if first_day.month() == 12 {
        NaiveDate::from_ymd_opt(first_day.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first_day.year(), first_day.month() + 1, 1)
    }
    .expect("a valid first of month");
    let last_day = i64::from(next_month.pred_opt().expect("a day before exists").day());
    first_day.with_day(due_day.min(last_day) as u32).expect("a valid day of month")
}

/// How a bill stands for a month, given today and what has been paid.
fn status_for(
    today: NaiveDate,
    due: NaiveDate,
    remind_days_before: i64,
    amount_cents: i64,
    paid_cents: i64,
) -> BillStatus {
    if paid_cents >= amount_cents {
        BillStatus::Paid
    } else if today > due {
        BillStatus::Overdue
    } else if (due - today).num_days() <= remind_days_before {
        BillStatus::Due
    } else {
        BillStatus::Upcoming
    }
}

async fn payee_names(db: &Db) -> AppResult<HashMap<String, String>> {
    Ok(payees::service::list_payees(db, PayeeListQuery::default())
        .await?
        .into_iter()
        .map(|p| (p.key, p.name))
        .collect())
}

fn to_dto(doc: &RecurringExpenseDocument, category_name: String, payee_name: Option<String>) -> RecurringExpense {
    RecurringExpense {
        id: doc.id.map(|o| o.to_hex()).unwrap_or_else(|| doc.key.clone()),
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

async fn dtos(db: &Db, docs: &[RecurringExpenseDocument]) -> AppResult<Vec<RecurringExpense>> {
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

/// Adds a regular bill. The category must count as a business cost; one bill per category and payee.
pub async fn create_template(db: &Db, body: CreateRecurringExpenseRequest) -> AppResult<RecurringExpense> {
    tracked("recurring_expenses.created", async move {
        check_schedule(body.amount_cents, body.due_day, body.remind_days_before, body.paid_from)?;
        let category = cash_categories::service::category_for_expense(db, &body.category_key).await?;
        check_payee(category.movement_type, body.paid_from, body.payee_key.as_deref())?;
        if let Some(payee_key) = &body.payee_key {
            payees::service::resolve_payee(db, payee_key).await?;
        }
        let key = recurring_expense_key(&category.key, body.payee_key.as_deref());
        if repository::find_by_key(db, &key).await?.is_some() {
            return Err(AppError::conflict(
                codes::RECURRING_EXPENSE_EXISTS,
                "There is already a regular bill for this. Change that one instead.",
            ));
        }
        let now = BsonDateTime::now();
        let inserted = repository::insert(
            db,
            RecurringExpenseDocument {
                id: None,
                key,
                category_key: category.key,
                payee_key: body.payee_key,
                amount_cents: body.amount_cents,
                due_day: body.due_day,
                remind_days_before: body.remind_days_before,
                paid_from: body.paid_from,
                note: clean_note(body.note.as_deref())?,
                active: true,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        Ok(dtos(db, std::slice::from_ref(&inserted)).await?.remove(0))
    })
    .await
}

/// Changes the amount, schedule, usual account or note of a regular bill, or stops or restarts it.
pub async fn update_template(
    db: &Db,
    key: &str,
    body: UpdateRecurringExpenseRequest,
) -> AppResult<RecurringExpense> {
    tracked("recurring_expenses.updated", async move {
        let mut doc = repository::find_by_key(db, key).await?.ok_or_else(|| {
            AppError::not_found_with_code("Regular bill not found.", codes::RECURRING_EXPENSE_NOT_FOUND)
        })?;
        doc.amount_cents = body.amount_cents.unwrap_or(doc.amount_cents);
        doc.due_day = body.due_day.unwrap_or(doc.due_day);
        doc.remind_days_before = body.remind_days_before.unwrap_or(doc.remind_days_before);
        doc.paid_from = body.paid_from.unwrap_or(doc.paid_from);
        check_schedule(doc.amount_cents, doc.due_day, doc.remind_days_before, doc.paid_from)?;
        // Only a change of account is checked, so a bill can always be stopped.
        if body.paid_from.is_some() {
            let category =
                cash_categories::service::category_for_expense(db, &doc.category_key).await?;
            check_payee(category.movement_type, doc.paid_from, doc.payee_key.as_deref())?;
        }
        if let Some(note) = &body.note {
            doc.note = clean_note(Some(note))?;
        }
        if let Some(active) = body.active {
            doc.active = active;
        }
        doc.version += 1;
        doc.updated_at = BsonDateTime::now();
        repository::save(db, &doc).await?;
        Ok(dtos(db, std::slice::from_ref(&doc)).await?.remove(0))
    })
    .await
}

/// Every regular bill with where it stands for the month that needs attention: the earliest month of
/// last month and this month that is not paid in full, else this month. Most urgent first.
///
/// shortcut: each template reads the whole expense list for its payments; a shop has a handful of
/// regular bills, so this stays cheap. Upgrade to one shared read if a shop keeps dozens.
pub async fn list_bills(db: &Db, query: RecurringExpenseListQuery) -> AppResult<Vec<RecurringBill>> {
    let include_inactive = query.include_inactive.unwrap_or(false);
    let docs: Vec<RecurringExpenseDocument> = repository::list_all(db)
        .await?
        .into_iter()
        .filter(|d| include_inactive || d.active)
        .collect();
    let templates = dtos(db, &docs).await?;
    let today = Utc::now().date_naive();
    let this_month = today.with_day(1).expect("day 1 exists");

    let mut bills = Vec::with_capacity(templates.len());
    for template in templates {
        let created = template.created_at.date_naive().with_day(1).expect("day 1 exists");
        // A bill added this month has nothing owed for last month.
        let candidates: Vec<NaiveDate> = [previous_month(this_month), this_month]
            .into_iter()
            .filter(|m| *m >= created)
            .collect();

        let mut chosen = None;
        for month in &candidates {
            let payments = expenses::service::period_payments(
                db,
                &template.category_key,
                template.payee_key.as_deref(),
                &month_string(*month),
            )
            .await?;
            let paid: i64 = payments.iter().map(|p| p.amount_cents).sum();
            if paid < template.amount_cents || chosen.is_none() {
                let unpaid = paid < template.amount_cents;
                chosen = Some((*month, payments, paid));
                if unpaid {
                    break;
                }
            }
        }
        let (month, payments, paid) = chosen.unwrap_or_else(|| (this_month, Vec::new(), 0));
        let due = due_date_in(month, template.due_day);
        bills.push(RecurringBill {
            status: status_for(today, due, template.remind_days_before, template.amount_cents, paid),
            period: month_string(month),
            due_date: due.format("%Y-%m-%d").to_string(),
            days_until_due: (due - today).num_days(),
            paid_cents: paid,
            balance_cents: (template.amount_cents - paid).max(0),
            payments,
            template,
        });
    }
    bills.sort_by_key(|b| b.days_until_due);
    Ok(bills)
}

/// What is already recorded for a category and payee in a month, for the "already paid" warning.
pub async fn period_status(db: &Db, query: PeriodStatusQuery) -> AppResult<PeriodStatus> {
    let month = parse_month(&query.period)?;
    let period = month_string(month);
    let payments = expenses::service::period_payments(
        db,
        &query.category_key,
        query.payee_key.as_deref(),
        &period,
    )
    .await?;
    let paid_cents: i64 = payments.iter().map(|p| p.amount_cents).sum();
    let template_key = recurring_expense_key(&query.category_key, query.payee_key.as_deref());
    let expected_cents = repository::find_by_key(db, &template_key)
        .await?
        .filter(|t| t.active)
        .map(|t| t.amount_cents);
    Ok(PeriodStatus {
        period,
        paid_cents,
        expected_cents,
        already_paid: match expected_cents {
            Some(expected) => paid_cents >= expected,
            None => paid_cents > 0,
        },
        payments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn due_day_falls_back_to_the_last_day_of_a_short_month() {
        assert_eq!(due_date_in(day(2026, 2, 1), 31), day(2026, 2, 28));
        assert_eq!(due_date_in(day(2028, 2, 1), 30), day(2028, 2, 29));
        assert_eq!(due_date_in(day(2026, 12, 1), 31), day(2026, 12, 31));
        assert_eq!(due_date_in(day(2026, 10, 1), 1), day(2026, 10, 1));
    }

    #[test]
    fn a_bill_moves_from_upcoming_to_due_to_overdue_and_paid_wins() {
        let due = day(2026, 10, 10);
        let s = |today, paid| status_for(today, due, 3, 1000, paid);
        assert_eq!(s(day(2026, 10, 1), 0), BillStatus::Upcoming);
        assert_eq!(s(day(2026, 10, 7), 0), BillStatus::Due);
        assert_eq!(s(day(2026, 10, 10), 0), BillStatus::Due);
        assert_eq!(s(day(2026, 10, 11), 0), BillStatus::Overdue);
        assert_eq!(s(day(2026, 10, 11), 400), BillStatus::Overdue);
        assert_eq!(s(day(2026, 10, 11), 1000), BillStatus::Paid);
    }

    #[test]
    fn schedules_are_checked() {
        assert!(check_schedule(100, 1, 0, PaidFrom::Bank).is_ok());
        assert!(check_schedule(0, 1, 0, PaidFrom::Bank).is_err());
        assert!(check_schedule(100, 0, 0, PaidFrom::Bank).is_err());
        assert!(check_schedule(100, 32, 0, PaidFrom::Bank).is_err());
        assert!(check_schedule(100, 5, 31, PaidFrom::Bank).is_err());
        assert!(check_schedule(100, 5, 3, PaidFrom::Unpaid).is_err());
    }

    #[test]
    fn drawer_rent_needs_a_payee_but_other_bills_do_not() {
        use CashMovementType::{PettyExpense, RentAndBills};
        assert!(check_payee(RentAndBills, PaidFrom::Drawer, None).is_err());
        assert!(check_payee(RentAndBills, PaidFrom::Drawer, Some("pye_x")).is_ok());
        assert!(check_payee(RentAndBills, PaidFrom::Bank, None).is_ok());
        assert!(check_payee(PettyExpense, PaidFrom::Drawer, None).is_ok());
    }

    #[test]
    fn months_step_back_across_a_year() {
        assert_eq!(previous_month(day(2026, 1, 1)), day(2025, 12, 1));
        assert_eq!(month_string(day(2026, 10, 1)), "2026-10");
        assert!(parse_month("2026-13").is_err());
    }
}
