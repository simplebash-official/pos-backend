// Business rules for the money reports: which day range is allowed and how each figure is put
// together from the ledgers. Nothing is stored; every call reads the current records.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate, Utc};

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::{
        accounts::AccountKind,
        cash_categories::{ReportGroup, category_key},
        expenses::PaidFrom,
        finance_reports::{
            AccountCashFlow, CashFlowLine, CashFlowResponse, CategoryBreakdown, CategoryTotal,
            ExpenseBreakdownResponse, ExpenseGroupTotal, MonthTotal, PayableBill, PayableSupplier, PayablesResponse,
            ProfitLossResponse,
        },
        incomes::ReceivedInto,
        purchase_receipts::ReceiptListQuery,
        sessions::CashMovementType,
    },
    modules::{balances, billing, cash_categories, expenses, incomes, purchases, reports, sessions},
};

/// The longest range a report covers, so one request cannot read years of records.
const MAX_DAYS: i64 = 732;
/// The cash drop that takes cash to the bank, and the add-cash that brings it back (see the accounts).
const BANK_DEPOSIT: &str = "To bank deposit";
const BANK_WITHDRAWAL: &str = "Bank withdrawal to drawer";

fn parse_day(label: &str, value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("Please pick a valid {label}.")))
}

/// Checks a `from`..`to` range of days and returns it as dates.
fn parse_range(from: &str, to: &str) -> AppResult<(NaiveDate, NaiveDate)> {
    let (start, end) = (parse_day("start date", from)?, parse_day("end date", to)?);
    if end < start {
        return Err(AppError::validation("The end date cannot be before the start date."));
    }
    if (end - start).num_days() > MAX_DAYS {
        return Err(AppError::validation("Please pick a period of at most two years."));
    }
    Ok((start, end))
}

fn day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn start_of(d: NaiveDate) -> chrono::DateTime<Utc> {
    d.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc()
}

/// Per category key: its name and its running total.
type CategorySums = BTreeMap<String, (String, i64)>;

fn sorted_totals(map: CategorySums) -> Vec<CategoryTotal> {
    let mut list: Vec<CategoryTotal> = map
        .into_iter()
        .map(|(key, (name, total))| CategoryTotal { category_key: key, category_name: name, total_cents: total })
        .filter(|c| c.total_cents != 0)
        .collect();
    list.sort_by(|a, b| b.total_cents.cmp(&a.total_cents).then(a.category_name.cmp(&b.category_name)));
    list
}

/// Profit and loss for a period: sales less returns and what the sold items cost, plus other income, less
/// running costs and staff pay. A cost counts on its own date whether or not it is paid yet; supplier stock
/// payments, owner drawings, cash moves and loan repayments are not costs (see `ReportGroup`).
pub async fn profit_loss(db: &Db, from: &str, to: &str) -> AppResult<ProfitLossResponse> {
    let (start, end) = parse_range(from, to)?;
    let after_end = end.succ_opt().ok_or_else(|| AppError::validation("Please pick a valid end date."))?;
    let (sales_cents, cost_of_goods_cents, refunds_cents) =
        reports::service::monthly_profit::sales_cost_and_refunds(db, start_of(start), start_of(after_end)).await?;
    let net_sales_cents = sales_cents - refunds_cents;
    let gross_profit_cents = net_sales_cents - cost_of_goods_cents;

    let mut income = CategorySums::new();
    for row in incomes::service::rows_between(db, from, to).await? {
        if row.report_group == ReportGroup::OtherIncome {
            income.entry(row.category_key).or_insert((row.category_name, 0)).1 += row.amount_cents;
        }
    }
    let other_income = sorted_totals(income);
    let other_income_cents = other_income.iter().map(|c| c.total_cents).sum::<i64>();

    let mut groups: BTreeMap<&'static str, (ReportGroup, CategorySums)> = BTreeMap::new();
    for row in expenses::service::rows_between(db, from, to).await? {
        if matches!(row.report_group, ReportGroup::OperatingExpense | ReportGroup::Payroll) {
            groups
                .entry(row.report_group.as_str())
                .or_insert((row.report_group, CategorySums::new()))
                .1
                .entry(row.category_key)
                .or_insert((row.category_name, 0))
                .1 += row.amount_cents;
        }
    }
    let expense_groups: Vec<ExpenseGroupTotal> = groups
        .into_values()
        .map(|(group, categories)| {
            let categories = sorted_totals(categories);
            ExpenseGroupTotal { group, total_cents: categories.iter().map(|c| c.total_cents).sum(), categories }
        })
        .collect();
    let expenses_cents = expense_groups.iter().map(|g| g.total_cents).sum::<i64>();

    Ok(ProfitLossResponse {
        from: day(start),
        to: day(end),
        sales_cents,
        refunds_cents,
        net_sales_cents,
        cost_of_goods_cents,
        gross_profit_cents,
        other_income,
        other_income_cents,
        expenses: expense_groups,
        expenses_cents,
        net_profit_cents: gross_profit_cents + other_income_cents - expenses_cents,
    })
}

/// Every `YYYY-MM` from the month of `start` to the month of `end`.
fn months_between(start: NaiveDate, end: NaiveDate) -> Vec<String> {
    let (mut y, mut m) = (start.year(), start.month());
    let mut out = Vec::new();
    while (y, m) <= (end.year(), end.month()) {
        out.push(format!("{y:04}-{m:02}"));
        if m == 12 {
            (y, m) = (y + 1, 1);
        } else {
            m += 1;
        }
    }
    out
}

/// What was spent per category and month. Includes supplier stock payments (they are money spent), and counts a
/// rent or pay payment in the month it covers, like the Regular bills do.
pub async fn expense_breakdown(db: &Db, from: &str, to: &str) -> AppResult<ExpenseBreakdownResponse> {
    let (start, end) = parse_range(from, to)?;
    let months = months_between(start, end);
    let mut by_category: BTreeMap<String, (String, ReportGroup, BTreeMap<String, i64>)> = BTreeMap::new();
    let mut by_month: BTreeMap<String, i64> = months.iter().map(|m| (m.clone(), 0)).collect();
    for row in expenses::service::rows_between(db, from, to).await? {
        let month = expenses::service::counted_month(&row).to_string();
        // A period the row names can fall outside the asked range (rent for last month paid today).
        let Some(slot) = by_month.get_mut(&month) else { continue };
        *slot += row.amount_cents;
        *by_category
            .entry(row.category_key)
            .or_insert((row.category_name, row.report_group, BTreeMap::new()))
            .2
            .entry(month)
            .or_insert(0) += row.amount_cents;
    }
    let mut categories: Vec<CategoryBreakdown> = by_category
        .into_iter()
        .map(|(key, (name, group, per_month))| CategoryBreakdown {
            category_key: key,
            category_name: name,
            report_group: group,
            total_cents: per_month.values().sum(),
            months: months
                .iter()
                .map(|m| MonthTotal { month: m.clone(), total_cents: per_month.get(m).copied().unwrap_or(0) })
                .collect(),
        })
        .collect();
    categories.sort_by(|a, b| b.total_cents.cmp(&a.total_cents).then(a.category_name.cmp(&b.category_name)));
    Ok(ExpenseBreakdownResponse {
        from: day(start),
        to: day(end),
        total_cents: by_month.values().sum(),
        categories,
        months: by_month.into_iter().map(|(month, total_cents)| MonthTotal { month, total_cents }).collect(),
    })
}

/// A cumulative figure between two days: what it was from `from` on, less what it was from the day after `to` on.
async fn between<T, F, Fut>(from: &str, after_to: &str, read: F) -> AppResult<(T, T)>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = AppResult<T>>,
{
    Ok((read(from.to_string()).await?, read(after_to.to_string()).await?))
}

fn line(code: &str, in_cents: i64, out_cents: i64) -> CashFlowLine {
    CashFlowLine { code: code.to_string(), in_cents, out_cents }
}

fn flow(kind: AccountKind, lines: Vec<CashFlowLine>) -> AccountCashFlow {
    let lines: Vec<CashFlowLine> = lines.into_iter().filter(|l| l.in_cents != 0 || l.out_cents != 0).collect();
    let (in_cents, out_cents) = (lines.iter().map(|l| l.in_cents).sum::<i64>(), lines.iter().map(|l| l.out_cents).sum::<i64>());
    AccountCashFlow { kind, in_cents, out_cents, net_cents: in_cents - out_cents, lines }
}

/// Money in and out of each account over a period, with the reasons. A cash drop to the bank is money out of the
/// drawer and money into the bank, so it shows on both. Card and online sales stay in their own accounts (the
/// shop's own settlement into the bank is not recorded).
pub async fn cash_flow(db: &Db, from: &str, to: &str) -> AppResult<CashFlowResponse> {
    let (start, end) = parse_range(from, to)?;
    let after_end = day(end.succ_opt().ok_or_else(|| AppError::validation("Please pick a valid end date."))?);
    let from_day = day(start);

    // Sales and refunds by method.
    let (a, b) = between(&from_day, &after_end, |d| async move {
        billing::service::payments::totals_by_method_since(db, &d).await
    })
    .await?;
    let sales = |method: &str| -> (i64, i64) {
        let pick = |rows: &[(String, i64, i64)]| {
            rows.iter().filter(|(m, _, _)| m == method).fold((0, 0), |acc, (_, r, f)| (acc.0 + r, acc.1 + f))
        };
        let (x, y) = (pick(&a), pick(&b));
        (x.0 - y.0, x.1 - y.1)
    };

    // Ledger payments and receipts by account.
    let (pa, pb) = between(&from_day, &after_end, |d| async move {
        expenses::service::account_payments_since(db, &d).await
    })
    .await?;
    let paid = |from_account: PaidFrom| -> i64 {
        let sum = |rows: &[(PaidFrom, i64)]| rows.iter().filter(|(f, _)| *f == from_account).map(|(_, c)| c).sum::<i64>();
        sum(&pa) - sum(&pb)
    };
    let (ra, rb) = between(&from_day, &after_end, |d| async move {
        incomes::service::account_receipts_since(db, &d).await
    })
    .await?;
    let received = |into: ReceivedInto| -> i64 {
        let sum = |rows: &[(ReceivedInto, i64)]| rows.iter().filter(|(i, _)| *i == into).map(|(_, c)| c).sum::<i64>();
        sum(&ra) - sum(&rb)
    };

    // The drawer's own movements, and the bank deposits among them.
    let deposit = cash_categories::service::keys_including_merged(db, &category_key(CashMovementType::CashDrop, BANK_DEPOSIT)).await?;
    let withdrawal =
        cash_categories::service::keys_including_merged(db, &category_key(CashMovementType::AddCash, BANK_WITHDRAWAL)).await?;
    let (mut cash_added, mut paid_out, mut owner, mut to_bank_or_safe) = (0, 0, 0, 0);
    let (mut deposited, mut withdrawn) = (0, 0);
    for m in sessions::service::movements_between(db, &from_day, &day(end)).await? {
        match m.movement_type {
            CashMovementType::AddCash => {
                cash_added += m.amount_cents;
                if withdrawal.contains(&m.category_key) {
                    withdrawn += m.amount_cents;
                }
            }
            CashMovementType::OwnerWithdrawal => owner += m.amount_cents,
            CashMovementType::CashDrop => {
                to_bank_or_safe += m.amount_cents;
                if deposit.contains(&m.category_key) {
                    deposited += m.amount_cents;
                }
            }
            _ => paid_out += m.amount_cents,
        }
    }

    let (cash_sales, cash_refunds) = sales("cash");
    let (card_sales, card_refunds) = sales("card");
    let (online_sales, online_refunds) = sales("online");
    Ok(CashFlowResponse {
        from: from_day,
        to: day(end),
        accounts: vec![
            flow(
                AccountKind::Drawer,
                vec![
                    line("sales", cash_sales, 0),
                    line("refunds", 0, cash_refunds),
                    line("cash_added", cash_added, 0),
                    line("paid_out", 0, paid_out),
                    line("owner_withdrawals", 0, owner),
                    line("cash_to_bank_or_safe", 0, to_bank_or_safe),
                ],
            ),
            flow(
                AccountKind::Bank,
                vec![
                    line("bank_deposits", deposited, 0),
                    line("other_income", received(ReceivedInto::Bank), 0),
                    line("expense_payments", 0, paid(PaidFrom::Bank)),
                    line("bank_withdrawals", 0, withdrawn),
                ],
            ),
            flow(
                AccountKind::Card,
                vec![
                    line("sales", card_sales, 0),
                    line("refunds", 0, card_refunds),
                    line("other_income", received(ReceivedInto::Card), 0),
                    line("expense_payments", 0, paid(PaidFrom::Card)),
                ],
            ),
            flow(
                AccountKind::Online,
                vec![
                    line("sales", online_sales, 0),
                    line("refunds", 0, online_refunds),
                    line("other_income", received(ReceivedInto::Online), 0),
                    line("expense_payments", 0, paid(PaidFrom::Online)),
                ],
            ),
        ],
    })
}

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
    let overdue_cents = bills.iter().filter(|b| b.is_overdue).map(|b| b.balance_cents).sum::<i64>();

    let mut suppliers: Vec<PayableSupplier> = balances::service::list_supplier_balances(db)
        .await?
        .suppliers
        .into_iter()
        .filter_map(|s| match s.balance_cents {
            Some(balance) if balance > 0 => {
                Some(PayableSupplier { supplier_key: s.supplier_key, name: s.name, balance_cents: balance })
            }
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
        suppliers.push(PayableSupplier { supplier_key, name, balance_cents });
    }
    suppliers.sort_by(|a, b| b.balance_cents.cmp(&a.balance_cents).then(a.name.cmp(&b.name)));
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
