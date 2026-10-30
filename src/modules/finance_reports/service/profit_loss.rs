use super::day::day;
use super::parse_range::parse_range;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::ReportGroup;
use crate::domain::finance_reports::{CategoryTotal, ExpenseGroupTotal, ProfitLossResponse};
use crate::modules::{expenses, incomes, reports};
use chrono::{NaiveDate, Utc};
use std::collections::BTreeMap;

fn start_of(d: NaiveDate) -> chrono::DateTime<Utc> {
    d.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc()
}

/// Per category key: its name and its running total.
type CategorySums = BTreeMap<String, (String, i64)>;

fn sorted_totals(map: CategorySums) -> Vec<CategoryTotal> {
    let mut list: Vec<CategoryTotal> = map
        .into_iter()
        .map(|(key, (name, total))| CategoryTotal {
            category_key: key,
            category_name: name,
            total_cents: total,
        })
        .filter(|c| c.total_cents != 0)
        .collect();
    list.sort_by(|a, b| {
        b.total_cents
            .cmp(&a.total_cents)
            .then(a.category_name.cmp(&b.category_name))
    });
    list
}

/// Profit and loss for a period: sales less returns and what the sold items cost, plus other income, less
/// running costs and staff pay. A cost counts on its own date whether or not it is paid yet; supplier stock
/// payments, owner drawings, cash moves and loan repayments are not costs (see `ReportGroup`).
pub async fn profit_loss(db: &Db, from: &str, to: &str) -> AppResult<ProfitLossResponse> {
    let (start, end) = parse_range(from, to)?;
    let after_end = end
        .succ_opt()
        .ok_or_else(|| AppError::validation("Please pick a valid end date."))?;
    let (sales_cents, cost_of_goods_cents, refunds_cents) =
        reports::service::monthly_profit::sales_cost_and_refunds(
            db,
            start_of(start),
            start_of(after_end),
        )
        .await?;
    let net_sales_cents = sales_cents - refunds_cents;
    let gross_profit_cents = net_sales_cents - cost_of_goods_cents;

    let mut income = CategorySums::new();
    for row in incomes::service::rows_between(db, from, to).await? {
        if row.report_group == ReportGroup::OtherIncome {
            income
                .entry(row.category_key)
                .or_insert((row.category_name, 0))
                .1 += row.amount_cents;
        }
    }
    let other_income = sorted_totals(income);
    let other_income_cents = other_income.iter().map(|c| c.total_cents).sum::<i64>();

    let mut groups: BTreeMap<&'static str, (ReportGroup, CategorySums)> = BTreeMap::new();
    for row in expenses::service::rows_between(db, from, to).await? {
        if matches!(
            row.report_group,
            ReportGroup::OperatingExpense | ReportGroup::Payroll
        ) {
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
            ExpenseGroupTotal {
                group,
                total_cents: categories.iter().map(|c| c.total_cents).sum(),
                categories,
            }
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
