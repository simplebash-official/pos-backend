// Business rules for the expense ledger: what can be entered here, how much is paid, and the
// combined list of ledger expenses and drawer payments.

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::{db::Db, uow::Tx},
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        logging::domain::tracked,
    },
    domain::{
        cash_categories::normalize_name,
        expenses::{
            CreateExpensePaymentRequest, CreateExpenseRequest, ExpenseDetail, ExpenseListQuery,
            ExpenseListResponse, ExpensePayment, ExpenseRow, ExpenseSource, ExpenseStats,
            ExpenseStatus, PaidFrom,
        },
        purchase_receipts::PaymentKind,
        recurring_expenses::PeriodPayment,
        sequences::ReserveSequenceRequest,
    },
    modules::{
        cash_categories::{self, service::ResolvedCategory},
        expenses::{
            model::{ExpenseDocument, ExpensePaymentDocument},
            repository,
        },
        payees,
        purchases::service::settlement,
        sequences,
        sessions::{self, model::CashMovementDocument},
    },
};

const TEXT_MAX: usize = 200;
const REFERENCE_MAX: usize = 60;
const NAME_MAX: usize = 60;

fn parse_day(label: &str, value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("Please pick a valid {label}.")))
}

fn clean(label: &str, value: Option<&str>, max: usize) -> AppResult<Option<String>> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > max {
        return Err(AppError::validation(format!(
            "{label} can have up to {max} letters."
        )));
    }
    Ok(Some(text.to_string()))
}

fn today() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

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

/// A ledger expense as a list row, with what its payments add up to.
fn ledger_row(
    e: &ExpenseDocument,
    payments: &[&ExpensePaymentDocument],
    category: &ResolvedCategory,
    today: &str,
) -> ExpenseRow {
    let paid_cents: i64 = payments.iter().map(|p| p.amount_cents).sum();
    let status = ExpenseStatus::of(e.amount_cents, paid_cents);
    ExpenseRow {
        key: e.key.clone(),
        source: ExpenseSource::Ledger,
        number: e.expense_number.clone(),
        date: e.expense_date.clone(),
        category_key: e.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: e.payee_key.clone(),
        payee_name: e.payee_name.clone(),
        amount_cents: e.amount_cents,
        paid_cents,
        balance_cents: e.amount_cents - paid_cents,
        status,
        paid_from: payments.last().map_or(PaidFrom::Unpaid, |p| p.paid_from),
        due_date: e.due_date.clone(),
        period_start: None,
        is_overdue: status != ExpenseStatus::Paid
            && e.due_date.as_deref().is_some_and(|due| due < today),
        reference_no: e.reference_no.clone(),
        note: e.note.clone(),
        movement_type: None,
        recorded_by_name_snapshot: e.recorded_by_name_snapshot.clone(),
        created_at: e.created_at.to_chrono(),
    }
}

/// A drawer payment as a list row. Always paid in full, from the drawer.
fn drawer_row(m: &CashMovementDocument, category: &ResolvedCategory) -> ExpenseRow {
    let created = m.created_at.to_chrono();
    ExpenseRow {
        key: m.key.clone(),
        source: ExpenseSource::Drawer,
        number: m.movement_number.clone(),
        date: created.format("%Y-%m-%d").to_string(),
        category_key: m.category_key.clone(),
        category_name: category.name.clone(),
        report_group: category.report_group,
        payee_key: m.payee_key.clone(),
        payee_name: m.payee_name.clone(),
        amount_cents: m.amount_cents,
        paid_cents: m.amount_cents,
        balance_cents: 0,
        status: ExpenseStatus::Paid,
        paid_from: PaidFrom::Drawer,
        due_date: None,
        period_start: m.period_start.clone(),
        is_overdue: false,
        reference_no: m.reference_no.clone(),
        note: m.note.clone(),
        movement_type: Some(m.movement_type),
        recorded_by_name_snapshot: m.recorded_by_name_snapshot.clone(),
        created_at: created,
    }
}

/// Every cost between two days, ledger and drawer together, newest first.
///
/// shortcut: built in memory from the whole ledger plus the drawer payments of the range; a
/// shop's costs fit comfortably, upgrade to a paged query when a shop passes tens of thousands.
async fn all_rows(db: &Db, from: Option<&str>, to: Option<&str>) -> AppResult<Vec<ExpenseRow>> {
    let expenses = repository::list_expenses(db, from, to).await?;
    let movements = sessions::service::expense_movements(db, from, to).await?;
    // A drawer payment that was reversed, and the reversal itself, cancel out and are not costs. The
    // reversal may be dated outside the range asked for, so it is looked up by key, not by date.
    let movement_keys: Vec<String> = movements.iter().map(|m| m.key.clone()).collect();
    let reversed = sessions::service::reversed_keys(db, &movement_keys).await?;
    let movements: Vec<CashMovementDocument> = movements
        .into_iter()
        .filter(|m| m.reverses_key.is_none() && !reversed.contains(&m.key))
        .collect();
    let payments = repository::list_all_payments(db).await?;
    let mut by_expense: HashMap<&str, Vec<&ExpensePaymentDocument>> = HashMap::new();
    for p in &payments {
        by_expense
            .entry(p.expense_key.as_str())
            .or_default()
            .push(p);
    }

    let mut keys: Vec<String> = expenses
        .iter()
        .map(|e| e.category_key.clone())
        .chain(movements.iter().map(|m| m.category_key.clone()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    let categories = cash_categories::service::resolve_categories(db, &keys).await?;

    let day = today();
    let mut rows: Vec<ExpenseRow> = expenses
        .iter()
        .map(|e| {
            let paid = by_expense
                .get(e.key.as_str())
                .map_or(&[][..], Vec::as_slice);
            ledger_row(e, paid, &categories[&e.category_key], &day)
        })
        .collect();
    // A drawer payment is a cost only when its category counts as one (not a loan instalment).
    rows.extend(
        movements
            .iter()
            .filter(|m| {
                cash_categories::service::is_expense_group(categories[&m.category_key].report_group)
            })
            .map(|m| drawer_row(m, &categories[&m.category_key])),
    );
    rows.sort_by(|a, b| b.date.cmp(&a.date).then(b.created_at.cmp(&a.created_at)));
    Ok(rows)
}

/// Every cost dated between two days (`YYYY-MM-DD`, inclusive), drawer payments included, for the money reports.
pub(crate) async fn rows_between(db: &Db, from: &str, to: &str) -> AppResult<Vec<ExpenseRow>> {
    all_rows(db, Some(from), Some(to)).await
}

/// Every cost with something still to pay, whenever it was dated, for the payables report.
pub(crate) async fn unpaid_rows(db: &Db) -> AppResult<Vec<ExpenseRow>> {
    Ok(all_rows(db, None, None)
        .await?
        .into_iter()
        .filter(|r| r.balance_cents > 0)
        .collect())
}

/// The month (`YYYY-MM`) a cost counts towards: the period a drawer payment names, else its own date.
pub(crate) fn counted_month(row: &ExpenseRow) -> &str {
    &row.period_start.as_deref().unwrap_or(&row.date)[..7]
}

/// What is already recorded for a category (and payee, when given) in one month, from the ledger and
/// the drawer, with reversed drawer payments left out. Used for regular bills and the "already paid" warning.
pub(crate) async fn period_payments(
    db: &Db,
    category_key: &str,
    payee_key: Option<&str>,
    month: &str,
) -> AppResult<Vec<PeriodPayment>> {
    let category_keys = cash_categories::service::keys_including_merged(db, category_key).await?;
    Ok(all_rows(db, None, None)
        .await?
        .into_iter()
        .filter(|r| r.paid_cents > 0)
        .filter(|r| category_keys.contains(&r.category_key))
        .filter(|r| payee_key.is_none_or(|p| r.payee_key.as_deref() == Some(p)))
        .filter(|r| counted_month(r) == month)
        .map(|r| PeriodPayment {
            number: r.number,
            date: r.date,
            amount_cents: r.paid_cents,
            paid_from: r.paid_from,
        })
        .collect())
}

/// One ledger payment, as the balances need it.
pub(crate) struct LedgerPayout {
    pub payee_key: Option<String>,
    pub category_key: String,
    /// `YYYY-MM-DD` the money left.
    pub day: String,
    pub number: String,
    pub amount_cents: i64,
}

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

/// What was paid out of the bank, a card or online on or after `since` (`YYYY-MM-DD`), as `(account, cents)`
/// per payment. Drawer payments are cash movements and not counted here.
pub(crate) async fn account_payments_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(PaidFrom, i64)>> {
    Ok(repository::list_all_payments(db)
        .await?
        .into_iter()
        .filter(|p| p.paid_on.as_str() >= since)
        .map(|p| (p.paid_from, p.amount_cents))
        .collect())
}

/// A page of costs with the filters of the Expenses screen.
pub async fn list_expenses(db: &Db, query: ExpenseListQuery) -> AppResult<ExpenseListResponse> {
    if let Some(from) = &query.from {
        parse_day("start date", from)?;
    }
    if let Some(to) = &query.to {
        parse_day("end date", to)?;
    }
    if let Some(status) = &query.status
        && !matches!(status.as_str(), "unpaid" | "overdue" | "paid")
    {
        return Err(AppError::validation("Unknown status."));
    }
    let source = match query.source.as_deref() {
        None => None,
        Some("ledger") => Some(ExpenseSource::Ledger),
        Some("drawer") => Some(ExpenseSource::Drawer),
        Some(_) => return Err(AppError::validation("Unknown source.")),
    };
    let category_keys = match &query.category_key {
        Some(key) => Some(cash_categories::service::keys_including_merged(db, key).await?),
        None => None,
    };
    let search = query
        .search
        .as_deref()
        .map(normalize_name)
        .filter(|s| !s.is_empty());

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).min(200);

    let rows: Vec<ExpenseRow> = all_rows(db, query.from.as_deref(), query.to.as_deref())
        .await?
        .into_iter()
        .filter(|r| source.is_none_or(|s| r.source == s))
        .filter(|r| {
            category_keys
                .as_ref()
                .is_none_or(|keys| keys.contains(&r.category_key))
        })
        .filter(|r| {
            query
                .payee_key
                .as_ref()
                .is_none_or(|k| r.payee_key.as_ref() == Some(k))
        })
        .filter(|r| match query.status.as_deref() {
            Some("unpaid") => r.balance_cents > 0,
            Some("overdue") => r.is_overdue,
            Some("paid") => r.status == ExpenseStatus::Paid,
            _ => true,
        })
        .filter(|r| {
            search.as_ref().is_none_or(|q| {
                [
                    Some(r.number.as_str()),
                    Some(r.category_name.as_str()),
                    r.payee_name.as_deref(),
                    r.reference_no.as_deref(),
                    r.note.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|text| normalize_name(text).contains(q.as_str()))
            })
        })
        .collect();

    let total = rows.len() as u64;
    let start = ((page - 1) * limit) as usize;
    Ok(ExpenseListResponse {
        expenses: rows.into_iter().skip(start).take(limit as usize).collect(),
        total,
        page,
        limit,
        total_pages: total.div_ceil(limit),
    })
}

/// Headline figures: this month's costs, and what is still owed.
pub async fn get_stats(db: &Db) -> AppResult<ExpenseStats> {
    let rows = all_rows(db, None, None).await?;
    let month = &today()[..7];
    let mut stats = ExpenseStats {
        this_month_cents: 0,
        this_month_drawer_cents: 0,
        unpaid_cents: 0,
        unpaid_count: 0,
        overdue_cents: 0,
        overdue_count: 0,
    };
    for r in &rows {
        if r.date.starts_with(month) {
            stats.this_month_cents += r.amount_cents;
            if r.source == ExpenseSource::Drawer {
                stats.this_month_drawer_cents += r.amount_cents;
            }
        }
        if r.balance_cents > 0 {
            stats.unpaid_cents += r.balance_cents;
            stats.unpaid_count += 1;
            if r.is_overdue {
                stats.overdue_cents += r.balance_cents;
                stats.overdue_count += 1;
            }
        }
    }
    Ok(stats)
}

async fn detail_of(db: &Db, expense: &ExpenseDocument) -> AppResult<ExpenseDetail> {
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

/// One ledger expense with its payments.
pub async fn get_expense(db: &Db, key: &str) -> AppResult<ExpenseDetail> {
    let expense = repository::find_expense_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Expense not found.", codes::EXPENSE_NOT_FOUND)
        })?;
    detail_of(db, &expense).await
}

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

/// A validated expense, and the payment that settles it when it is paid in full on entry.
pub(crate) struct PreparedExpense {
    expense: ExpenseDocument,
    payment: Option<ExpensePaymentDocument>,
}

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

impl PreparedExpense {
    /// The key of the payment row, when the expense is paid in full on entry.
    pub(crate) fn payment_key(&self) -> Option<&str> {
        self.payment.as_ref().map(|p| p.key.as_str())
    }
}

/// Everything `create_expense` checks, with nothing written: the account, the amount, the dates,
/// the category, the payee, and the expense number taken.
pub(crate) async fn prepare_expense(
    db: &Db,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateExpenseRequest,
    device_id: Option<String>,
) -> AppResult<PreparedExpense> {
    {
        if body.paid_from == PaidFrom::Drawer {
            return Err(AppError::validation(
                "Cash paid from the drawer is recorded as a cash movement. It appears here by itself.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation(
                "Please enter an amount greater than zero.",
            ));
        }
        parse_day("expense date", &body.expense_date)?;
        let due_date = clean("The due date", body.due_date.as_deref(), 10)?;
        if let Some(due) = &due_date {
            if body.paid_from != PaidFrom::Unpaid {
                return Err(AppError::validation(
                    "A due date only applies to a bill that is not paid yet.",
                ));
            }
            parse_day("due date", due)?;
        }
        let category =
            cash_categories::service::category_for_expense(db, &body.category_key).await?;

        let typed = clean("The name", body.payee_name.as_deref(), NAME_MAX)?;
        let (payee_key, payee_name) = match (&body.payee_key, typed) {
            (Some(key), _) => {
                let resolved = payees::service::resolve_payee(db, key).await?;
                (Some(key.clone()), Some(resolved.name))
            }
            (None, Some(name)) => {
                let saved = payees::service::ensure_other_payee(db, &name).await?;
                (Some(saved.key), Some(saved.name))
            }
            (None, None) => (None, None),
        };

        let reservation = sequences::service::reserve_sequence(
            db,
            "expense".to_string(),
            ReserveSequenceRequest {
                block_size: Some(1),
                device_id,
            },
        )
        .await?;
        let expense_number = format!(
            "{}{:0width$}",
            reservation.prefix,
            reservation.start,
            width = reservation.padding
        );

        let now = BsonDateTime::now();
        let expense = ExpenseDocument {
            id: None,
            key: generate_id(prefixes::EXPENSE),
            expense_number,
            expense_date: body.expense_date.trim().to_string(),
            category_key: category.key,
            payee_key,
            payee_name,
            amount_cents: body.amount_cents,
            due_date,
            reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
            note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
            recorded_by_user_id: recorded_by_user_id.clone(),
            recorded_by_name_snapshot: recorded_by_name.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        };

        let payment = body.paid_from.is_account().then(|| ExpensePaymentDocument {
            id: None,
            key: generate_id(prefixes::EXPENSE_PAYMENT),
            expense_key: expense.key.clone(),
            amount_cents: expense.amount_cents,
            paid_from: body.paid_from,
            paid_on: expense.expense_date.clone(),
            reference_no: expense.reference_no.clone(),
            note: None,
            recorded_by_user_id,
            recorded_by_name_snapshot: recorded_by_name,
            version: 1,
            created_at: now,
            updated_at: now,
            updated_by_device: None,
        });
        Ok(PreparedExpense { expense, payment })
    }
}

/// Records a payment against a bill, from the bank, a card or online. Never more than is still owed.
pub async fn add_payment(
    db: &Db,
    expense_key: &str,
    recorded_by_user_id: String,
    recorded_by_name: String,
    body: CreateExpensePaymentRequest,
) -> AppResult<ExpenseDetail> {
    tracked("expenses.payment_recorded", async move {
        if !body.paid_from.is_account() || body.paid_from == PaidFrom::Drawer {
            return Err(AppError::validation(
                "Pick the bank, a card or online. Cash from the drawer is recorded as a cash movement.",
            ));
        }
        if body.amount_cents <= 0 {
            return Err(AppError::validation("Please enter an amount greater than zero."));
        }
        parse_day("payment date", &body.paid_on)?;
        let expense = repository::find_expense_by_key(db, expense_key).await?.ok_or_else(|| {
            AppError::not_found_with_code("Expense not found.", codes::EXPENSE_NOT_FOUND)
        })?;
        let paid: i64 = repository::list_payments_for(db, expense_key)
            .await?
            .iter()
            .map(|p| p.amount_cents)
            .sum();
        if body.amount_cents > expense.amount_cents - paid {
            return Err(AppError::conflict(
                codes::EXPENSE_OVERPAID,
                "This is more than is still owed on the bill.",
            ));
        }
        let now = BsonDateTime::now();
        repository::insert_payment(
            db,
            ExpensePaymentDocument {
                id: None,
                key: generate_id(prefixes::EXPENSE_PAYMENT),
                expense_key: expense.key.clone(),
                amount_cents: body.amount_cents,
                paid_from: body.paid_from,
                paid_on: body.paid_on.trim().to_string(),
                reference_no: clean("The reference", body.reference_no.as_deref(), REFERENCE_MAX)?,
                note: clean("The note", body.note.as_deref(), TEXT_MAX)?,
                recorded_by_user_id,
                recorded_by_name_snapshot: recorded_by_name,
                version: 1,
                created_at: now,
                updated_at: now,
                updated_by_device: None,
            },
        )
        .await?;
        detail_of(db, &expense).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_what_is_paid() {
        assert_eq!(ExpenseStatus::of(1000, 0), ExpenseStatus::Unpaid);
        assert_eq!(ExpenseStatus::of(1000, 400), ExpenseStatus::PartPaid);
        assert_eq!(ExpenseStatus::of(1000, 1000), ExpenseStatus::Paid);
    }

    #[test]
    fn days_must_be_real_dates() {
        assert!(parse_day("date", "2026-10-23").is_ok());
        assert!(parse_day("date", "2026-02-30").is_err());
        assert!(parse_day("date", "23/10/2026").is_err());
    }
}
