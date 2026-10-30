use super::dtos::dtos;
use super::due_date_in::due_date_in;
use super::month_string::month_string;
use super::previous_month::previous_month;
use super::status_for::status_for;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::recurring_expenses::{RecurringBill, RecurringExpenseListQuery};
use crate::modules::expenses;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use crate::modules::recurring_expenses::repository;
use chrono::{Datelike, NaiveDate, Utc};

/// Every regular bill with where it stands for the month that needs attention: the earliest month of
/// last month and this month that is not paid in full, else this month. Most urgent first.
///
/// shortcut: each template reads the whole expense list for its payments; a shop has a handful of
/// regular bills, so this stays cheap. Upgrade to one shared read if a shop keeps dozens.
pub async fn list_bills(
    db: &Db,
    query: RecurringExpenseListQuery,
) -> AppResult<Vec<RecurringBill>> {
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
        let created = template
            .created_at
            .date_naive()
            .with_day(1)
            .expect("day 1 exists");
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
            status: status_for(
                today,
                due,
                template.remind_days_before,
                template.amount_cents,
                paid,
            ),
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
