use super::day::day;
use super::parse_range::parse_range;
use crate::clients::db::Db;
use crate::core::calculations::share_basis_points;
use crate::core::error::AppResult;
use crate::domain::cash_categories::ReportGroup;
use crate::domain::finance_reports::{
    CategoryBreakdown, ChartSeries, ExpenseBreakdownResponse, MonthTotal,
};
use crate::modules::expenses;
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;

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

/// How many of the biggest categories get their own band in the monthly chart; the rest join one "other" band.
const CHART_CATEGORIES: usize = 5;

/// What was spent per category and month. Includes supplier stock payments (they are money spent), and counts a
/// rent or pay payment in the month it covers, like the Regular bills do.
pub async fn expense_breakdown(
    db: &Db,
    from: &str,
    to: &str,
) -> AppResult<ExpenseBreakdownResponse> {
    let (start, end) = parse_range(from, to)?;
    let months = months_between(start, end);
    let mut by_category: BTreeMap<String, (String, ReportGroup, BTreeMap<String, i64>)> =
        BTreeMap::new();
    let mut by_month: BTreeMap<String, i64> = months.iter().map(|m| (m.clone(), 0)).collect();
    for row in expenses::service::rows_between(db, from, to).await? {
        let month = expenses::service::counted_month(&row).to_string();
        // A period the row names can fall outside the asked range (rent for last month paid today).
        let Some(slot) = by_month.get_mut(&month) else {
            continue;
        };
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
            share_percent: 0,
            months: months
                .iter()
                .map(|m| MonthTotal {
                    month: m.clone(),
                    total_cents: per_month.get(m).copied().unwrap_or(0),
                })
                .collect(),
        })
        .collect();
    categories.sort_by(|a, b| {
        b.total_cents
            .cmp(&a.total_cents)
            .then(a.category_name.cmp(&b.category_name))
    });
    let total_cents: i64 = by_month.values().sum();
    for c in &mut categories {
        c.share_percent = (share_basis_points(c.total_cents, total_cents) + 50) / 100;
    }
    let mut chart: Vec<ChartSeries> = categories
        .iter()
        .take(CHART_CATEGORIES)
        .map(|c| ChartSeries {
            name: c.category_name.clone(),
            is_other: false,
            months: c.months.clone(),
        })
        .collect();
    if categories.len() > CHART_CATEGORIES {
        let months = (0..months.len())
            .map(|i| MonthTotal {
                month: months[i].clone(),
                total_cents: categories[CHART_CATEGORIES..]
                    .iter()
                    .map(|c| c.months[i].total_cents)
                    .sum(),
            })
            .collect();
        chart.push(ChartSeries {
            name: String::new(),
            is_other: true,
            months,
        });
    }
    Ok(ExpenseBreakdownResponse {
        from: day(start),
        to: day(end),
        total_cents,
        chart,
        categories,
        months: by_month
            .into_iter()
            .map(|(month, total_cents)| MonthTotal { month, total_cents })
            .collect(),
    })
}
