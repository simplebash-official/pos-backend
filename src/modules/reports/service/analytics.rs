// Orchestration for the Analytics & Reports endpoints. Resolves the
// shop-local date range and bucket granularity, fans out to the
// `repository::analytics` pipelines plus the existing commission/refund
// aggregations, and assembles the gap-filled, derived-figure responses the
// `/reports/analytics/*` handlers return.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};
use mongodb::Database;

use super::dates::{self, report_tz};
use crate::{
    core::error::AppResult,
    domain::reports::{
        AgingBucket, AgingDebtor, AnalyticsKpiDeltas, AnalyticsKpis,
        AnalyticsPaymentMethodsResponse, AnalyticsRangeQuery, AnalyticsSummaryResponse,
        CashierPerformanceEntry, CashierPerformanceResponse, CategorySalesRow,
        DiscountAnalyticsResponse, DiscountCashierRow, DiscountProductRow, DiscountTypeRow,
        Granularity, PatternBucket, PatternCell, ReceivablesAgingQuery, ReceivablesAgingResponse,
        RefundAnalyticsResponse, RefundMethodRow, RefundProductRow, RefundReasonRow,
        SalesByCategoryQuery, SalesByCategoryResponse, SalesPatternsResponse, TimeSeriesPoint,
        TimeSeriesResponse, TopCustomerEntry, TopCustomersQuery, TopCustomersResponse,
    },
    modules::reports::repository,
};

/// 1% == 100 bps. Returns `num/den` as basis points, or 0 when `den` is 0.
fn bps(num: i64, den: i64) -> i64 {
    if den == 0 {
        0
    } else {
        ((num as i128 * 10_000) / den as i128) as i64
    }
}

/// Percentage change `(cur - prev) / prev` in bps, `None` when `prev` is 0.
fn delta_bps(cur: i64, prev: i64) -> Option<i64> {
    if prev == 0 {
        None
    } else {
        Some((((cur - prev) as i128 * 10_000) / prev as i128) as i64)
    }
}

fn resolve_granularity(
    explicit: Option<&str>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Granularity> {
    match explicit.map(|g| g.to_lowercase()).as_deref() {
        Some("day") => Ok(Granularity::Day),
        Some("week") => Ok(Granularity::Week),
        Some("month") => Ok(Granularity::Month),
        Some("year") => Ok(Granularity::Year),
        Some(other) => Err(crate::core::error::AppError::validation(format!(
            "Unknown granularity '{other}'. Use day, week, month or year."
        ))),
        None => {
            let days = (end - start).num_days();
            Ok(if days <= 31 {
                Granularity::Day
            } else if days <= 366 {
                Granularity::Week
            } else if days <= 1096 {
                Granularity::Month
            } else {
                Granularity::Year
            })
        }
    }
}

/// Align an instant down to the start of its bucket, in shop-local time.
fn align_bucket(instant: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> DateTime<Utc> {
    let local = instant.with_timezone(&tz);
    let date = local.date_naive();
    let aligned_date = match gran {
        Granularity::Day => date,
        Granularity::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        Granularity::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap(),
        Granularity::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap(),
    };
    aligned_date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(tz)
        .single()
        .unwrap()
        .with_timezone(&Utc)
}

fn next_bucket(bucket_start: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> DateTime<Utc> {
    let local = bucket_start.with_timezone(&tz);
    let date = local.date_naive();
    let next_date = match gran {
        Granularity::Day => date + Duration::days(1),
        Granularity::Week => date + Duration::days(7),
        Granularity::Month => {
            if date.month() == 12 {
                NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap()
            }
        }
        Granularity::Year => NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap(),
    };
    next_date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(tz)
        .single()
        .unwrap()
        .with_timezone(&Utc)
}

fn bucket_label(bucket_start: DateTime<Utc>, gran: Granularity, tz: FixedOffset) -> String {
    let local = bucket_start.with_timezone(&tz);
    match gran {
        Granularity::Day => local.format("%-d %b").to_string(),
        Granularity::Week => format!("W{:02}", local.iso_week().week()),
        Granularity::Month => local.format("%Y-%m").to_string(),
        Granularity::Year => local.format("%Y").to_string(),
    }
}

// ---------------------------------------------------------------------------
// summary
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_summary(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<AnalyticsSummaryResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let (current, previous, deltas) = if query.compare_previous.unwrap_or(false) {
        let span = end - start;
        let prev_start = start - span;
        let (cur, prev) = tokio::try_join!(
            compute_kpis(db, start, end),
            compute_kpis(db, prev_start, start),
        )?;
        let deltas = AnalyticsKpiDeltas {
            total_revenue_bps: delta_bps(cur.total_revenue_cents, prev.total_revenue_cents),
            gross_profit_bps: delta_bps(cur.gross_profit_cents, prev.gross_profit_cents),
            net_profit_bps: delta_bps(cur.net_profit_cents, prev.net_profit_cents),
            invoice_count_bps: delta_bps(cur.invoice_count as i64, prev.invoice_count as i64),
            avg_basket_bps: delta_bps(cur.avg_basket_cents, prev.avg_basket_cents),
            gross_margin_delta_bps: cur.gross_margin_bps - prev.gross_margin_bps,
        };
        (cur, Some(prev), Some(deltas))
    } else {
        let cur = compute_kpis(db, start, end).await?;
        (cur, None, None)
    };

    Ok(AnalyticsSummaryResponse {
        period_start: start,
        period_end: end,
        current,
        previous,
        deltas,
    })
}

/// The full KPI block for one `[start, end)` window.
async fn compute_kpis(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<AnalyticsKpis> {
    let (invoice, commissions, refunds) = tokio::try_join!(
        repository::analytics::aggregate_invoice_kpis(db, start, end),
        repository::commissions::aggregate_employee_commissions(db, start, end, None),
        repository::refunds::aggregate_refund_totals(db, start, end),
    )?;

    let mut service_material_cost_cents = 0;
    let mut commission_payouts_cents = 0;
    for emp in commissions.values() {
        service_material_cost_cents += emp.estimated_cost_cents;
        commission_payouts_cents += emp.earned_commission_cents;
    }

    let total_revenue_cents = invoice.net_sales_cents;
    let total_cogs_cents = invoice.retail_cogs_cents + service_material_cost_cents;
    let gross_profit_cents = total_revenue_cents - total_cogs_cents;
    let net_profit_cents = gross_profit_cents - commission_payouts_cents - refunds.net_refund_cents;

    Ok(AnalyticsKpis {
        total_revenue_cents,
        retail_revenue_cents: invoice.retail_revenue_cents,
        repair_revenue_cents: invoice.repair_revenue_cents,
        print_revenue_cents: invoice.print_revenue_cents,
        invoice_count: invoice.invoice_count,
        items_sold: invoice.retail_items_sold,
        avg_basket_cents: if invoice.invoice_count == 0 {
            0
        } else {
            total_revenue_cents / invoice.invoice_count as i64
        },
        discount_cents: invoice.discount_cents,
        discount_rate_bps: bps(invoice.discount_cents, invoice.gross_sales_cents),
        retail_cogs_cents: invoice.retail_cogs_cents,
        service_material_cost_cents,
        gross_profit_cents,
        gross_margin_bps: bps(gross_profit_cents, total_revenue_cents),
        commission_payouts_cents,
        refunds_cents: refunds.net_refund_cents,
        refund_rate_bps: bps(refunds.net_refund_cents, total_revenue_cents),
        net_profit_cents,
        cogs_coverage_bps: bps(
            invoice.retail_revenue_with_cost_cents,
            invoice.retail_revenue_cents,
        ),
    })
}

// ---------------------------------------------------------------------------
// timeseries
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_timeseries(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<TimeSeriesResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let gran = resolve_granularity(query.granularity.as_deref(), start, end)?;
    let tz = report_tz();

    let (raw, commission) = tokio::try_join!(
        repository::analytics::aggregate_revenue_series(db, start, end, gran),
        repository::analytics::aggregate_commission_series(db, start, end, gran),
    )?;

    let raw_by_ms: std::collections::BTreeMap<i64, _> = raw
        .into_iter()
        .map(|b| (b.period_start.timestamp_millis(), b))
        .collect();

    let mut points = Vec::new();
    let mut cursor = align_bucket(start, gran, tz);
    // Guard against a pathological range producing an unbounded loop.
    let mut guard = 0;
    while cursor < end && guard < 5_000 {
        guard += 1;
        let ms = cursor.timestamp_millis();
        let commission_cents = commission.get(&ms).copied().unwrap_or(0);

        let point = if let Some(b) = raw_by_ms.get(&ms) {
            let revenue = b.retail_revenue_cents + b.repair_revenue_cents + b.print_revenue_cents;
            let gross_profit = revenue - b.cogs_cents;
            TimeSeriesPoint {
                period_start: cursor,
                label: bucket_label(cursor, gran, tz),
                revenue_cents: revenue,
                retail_revenue_cents: b.retail_revenue_cents,
                repair_revenue_cents: b.repair_revenue_cents,
                print_revenue_cents: b.print_revenue_cents,
                discount_cents: b.discount_cents,
                cogs_cents: b.cogs_cents,
                gross_profit_cents: gross_profit,
                gross_margin_bps: bps(gross_profit, revenue),
                commission_cents,
                net_profit_cents: gross_profit - commission_cents,
                invoice_count: b.invoice_count,
            }
        } else {
            TimeSeriesPoint {
                period_start: cursor,
                label: bucket_label(cursor, gran, tz),
                commission_cents,
                net_profit_cents: -commission_cents,
                ..Default::default()
            }
        };
        points.push(point);
        cursor = next_bucket(cursor, gran, tz);
    }

    let totals = fold_totals(&points);

    Ok(TimeSeriesResponse {
        granularity: gran.as_mongo_unit().to_string(),
        period_start: start,
        period_end: end,
        points,
        totals,
    })
}

fn fold_totals(points: &[TimeSeriesPoint]) -> TimeSeriesPoint {
    let mut t = TimeSeriesPoint {
        label: "Total".to_string(),
        ..Default::default()
    };
    for p in points {
        t.revenue_cents += p.revenue_cents;
        t.retail_revenue_cents += p.retail_revenue_cents;
        t.repair_revenue_cents += p.repair_revenue_cents;
        t.print_revenue_cents += p.print_revenue_cents;
        t.discount_cents += p.discount_cents;
        t.cogs_cents += p.cogs_cents;
        t.gross_profit_cents += p.gross_profit_cents;
        t.commission_cents += p.commission_cents;
        t.net_profit_cents += p.net_profit_cents;
        t.invoice_count += p.invoice_count;
    }
    t.gross_margin_bps = bps(t.gross_profit_cents, t.revenue_cents);
    if let Some(first) = points.first() {
        t.period_start = first.period_start;
    }
    t
}

/// Resolve `[start, end)` from an `AnalyticsRangeQuery` (shop-local).
fn range(q: &AnalyticsRangeQuery) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    dates::parse_date_range_tz(q.preset.as_deref(), q.from.as_deref(), q.to.as_deref())
}

// ---------------------------------------------------------------------------
// payment methods
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_payment_methods(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<AnalyticsPaymentMethodsResponse> {
    let (start, end) = range(&query)?;
    let (breakdown, total_cents) =
        repository::analytics::aggregate_payment_methods(db, start, end).await?;
    Ok(AnalyticsPaymentMethodsResponse {
        period_start: start,
        period_end: end,
        breakdown,
        total_cents,
    })
}

// ---------------------------------------------------------------------------
// top customers
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_top_customers(
    db: &Database,
    query: TopCustomersQuery,
) -> AppResult<TopCustomersResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let limit = query.limit.unwrap_or(100).clamp(1, 100) as usize;
    let sort_by = query.sort_by.as_deref().unwrap_or("revenue");

    let mut rows = repository::analytics::aggregate_top_customers(db, start, end).await?;
    let total_customers = rows.len() as u64;
    let total_revenue_cents: i64 = rows.iter().map(|r| r.revenue_cents).sum();

    rows.sort_by(|a, b| match sort_by {
        "invoices" => b.invoice_count.cmp(&a.invoice_count),
        "profit" => b.gross_profit_cents.cmp(&a.gross_profit_cents),
        _ => b.revenue_cents.cmp(&a.revenue_cents),
    });
    rows.truncate(limit);

    let keys: Vec<String> = rows.iter().filter_map(|r| r.customer_key.clone()).collect();
    let balances = repository::analytics::fetch_customer_balances(db, &keys).await?;

    let customers = rows
        .into_iter()
        .map(|r| {
            let outstanding_cents = r
                .customer_key
                .as_ref()
                .and_then(|k| balances.get(k))
                .copied()
                .unwrap_or(0);
            TopCustomerEntry {
                is_walk_in: r.customer_key.is_none(),
                customer_key: r.customer_key,
                customer_name: r.customer_name,
                customer_phone: r.customer_phone,
                invoice_count: r.invoice_count,
                revenue_cents: r.revenue_cents,
                discount_cents: r.discount_cents,
                gross_profit_cents: r.gross_profit_cents,
                outstanding_cents,
                last_purchase_at: r.last_purchase_at,
            }
        })
        .collect();

    Ok(TopCustomersResponse {
        customers,
        total_customers,
        total_revenue_cents,
    })
}

// ---------------------------------------------------------------------------
// sales by category
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_sales_by_category(
    db: &Database,
    query: SalesByCategoryQuery,
) -> AppResult<SalesByCategoryResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let by_subcategory = query.group_by.as_deref() == Some("subcategory");

    let raw =
        repository::analytics::aggregate_sales_by_category(db, start, end, by_subcategory).await?;

    let mut rows = Vec::new();
    let mut uncategorised = CategorySalesRow {
        category_name: "Uncategorised".to_string(),
        ..Default::default()
    };
    let mut total_revenue_cents = 0;
    let mut total_gross_profit_cents = 0;

    for r in raw {
        let gross_profit = r.revenue_cents - r.cogs_cents;
        total_revenue_cents += r.revenue_cents;
        total_gross_profit_cents += gross_profit;
        let row = CategorySalesRow {
            category_key: r.category_key.clone(),
            category_name: r.category_name,
            subcategory_key: r.subcategory_key,
            subcategory_name: r.subcategory_name,
            units_sold: r.units_sold,
            revenue_cents: r.revenue_cents,
            discount_cents: r.discount_cents,
            cogs_cents: r.cogs_cents,
            gross_profit_cents: gross_profit,
            gross_margin_bps: bps(gross_profit, r.revenue_cents),
        };
        if r.category_key.is_none() {
            uncategorised.units_sold += row.units_sold;
            uncategorised.revenue_cents += row.revenue_cents;
            uncategorised.discount_cents += row.discount_cents;
            uncategorised.cogs_cents += row.cogs_cents;
            uncategorised.gross_profit_cents += row.gross_profit_cents;
        } else {
            rows.push(row);
        }
    }
    uncategorised.gross_margin_bps = bps(
        uncategorised.gross_profit_cents,
        uncategorised.revenue_cents,
    );
    rows.sort_by_key(|r| std::cmp::Reverse(r.revenue_cents));

    Ok(SalesByCategoryResponse {
        rows,
        uncategorised,
        total_revenue_cents,
        total_gross_profit_cents,
    })
}

// ---------------------------------------------------------------------------
// cashier performance
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_cashier_performance(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<CashierPerformanceResponse> {
    let (start, end) = range(&query)?;
    let rows = repository::analytics::aggregate_cashier_performance(db, start, end).await?;
    let refunds = repository::analytics::aggregate_cashier_refunds(db, start, end).await?;

    let total_revenue_cents: i64 = rows.iter().map(|r| r.revenue_cents).sum();
    let total_invoices: u64 = rows.iter().map(|r| r.invoice_count).sum();

    let mut cashiers: Vec<CashierPerformanceEntry> = rows
        .into_iter()
        .map(|r| {
            let (refund_count, refunded_cents) =
                refunds.get(&r.cashier_id).copied().unwrap_or((0, 0));
            let gross_before = r.revenue_cents + r.discount_given_cents;
            CashierPerformanceEntry {
                discount_rate_bps: bps(r.discount_given_cents, gross_before),
                avg_basket_cents: if r.invoice_count == 0 {
                    0
                } else {
                    r.revenue_cents / r.invoice_count as i64
                },
                cashier_id: r.cashier_id,
                cashier_name: r.cashier_name,
                invoice_count: r.invoice_count,
                revenue_cents: r.revenue_cents,
                retail_revenue_cents: r.retail_revenue_cents,
                items_sold: r.items_sold,
                discount_given_cents: r.discount_given_cents,
                credit_invoice_count: r.credit_invoice_count,
                refund_count,
                refunded_cents,
                gross_profit_cents: r.gross_profit_cents,
            }
        })
        .collect();
    cashiers.sort_by_key(|c| std::cmp::Reverse(c.revenue_cents));

    Ok(CashierPerformanceResponse {
        cashiers,
        total_revenue_cents,
        total_invoices,
    })
}

// ---------------------------------------------------------------------------
// sales patterns
// ---------------------------------------------------------------------------

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub(crate) async fn get_analytics_sales_patterns(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<SalesPatternsResponse> {
    let (start, end) = range(&query)?;
    let raw = repository::analytics::aggregate_sales_patterns(db, start, end).await?;

    // Zero-filled 7×24 grid, row-major.
    let mut cells: Vec<PatternCell> = (1..=7u8)
        .flat_map(|w| {
            (0..24u8).map(move |h| PatternCell {
                weekday: w,
                hour: h,
                invoice_count: 0,
                revenue_cents: 0,
            })
        })
        .collect();

    let mut by_weekday = [(0u64, 0i64); 7];
    let mut by_hour = [(0u64, 0i64); 24];

    for (weekday, hour, count, revenue) in raw {
        let idx = ((weekday - 1) as usize) * 24 + hour as usize;
        if let Some(cell) = cells.get_mut(idx) {
            cell.invoice_count += count;
            cell.revenue_cents += revenue;
        }
        let w = &mut by_weekday[(weekday - 1) as usize];
        w.0 += count;
        w.1 += revenue;
        let h = &mut by_hour[hour as usize];
        h.0 += count;
        h.1 += revenue;
    }

    let busiest = cells
        .iter()
        .cloned()
        .max_by_key(|c| c.revenue_cents)
        .unwrap_or_default();

    Ok(SalesPatternsResponse {
        period_start: start,
        period_end: end,
        by_weekday: by_weekday
            .iter()
            .enumerate()
            .map(|(i, (count, rev))| PatternBucket {
                bucket: (i + 1) as u8,
                label: WEEKDAY_LABELS[i].to_string(),
                invoice_count: *count,
                revenue_cents: *rev,
            })
            .collect(),
        by_hour: by_hour
            .iter()
            .enumerate()
            .map(|(i, (count, rev))| PatternBucket {
                bucket: i as u8,
                label: format!("{i:02}:00"),
                invoice_count: *count,
                revenue_cents: *rev,
            })
            .collect(),
        cells,
        busiest,
    })
}

// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_receivables_aging(
    db: &Database,
    query: ReceivablesAgingQuery,
) -> AppResult<ReceivablesAgingResponse> {
    let as_of = match query.as_of.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => dates::parse_iso_or_ymd(s.trim(), true)?,
        None => Utc::now(),
    };

    let invoices = repository::analytics::aggregate_receivables_aging(db, as_of).await?;

    // [min, max] day boundaries; max None == open-ended.
    let defs: [(&str, i32, Option<i32>); 5] = [
        ("Current", i32::MIN, Some(0)),
        ("1–30 days", 1, Some(30)),
        ("31–60 days", 31, Some(60)),
        ("61–90 days", 61, Some(90)),
        ("90+ days", 91, None),
    ];
    let mut buckets: Vec<AgingBucket> = defs
        .iter()
        .map(|(label, min, max)| AgingBucket {
            label: label.to_string(),
            min_days: if *min == i32::MIN { 0 } else { *min },
            max_days: *max,
            invoice_count: 0,
            amount_cents: 0,
        })
        .collect();

    let mut total_outstanding_cents = 0;
    let mut debtors: std::collections::HashMap<Option<String>, AgingDebtor> =
        std::collections::HashMap::new();

    for inv in &invoices {
        total_outstanding_cents += inv.balance_due_cents;
        let bi = defs
            .iter()
            .position(|(_, min, max)| {
                inv.days_overdue >= *min && max.map(|m| inv.days_overdue <= m).unwrap_or(true)
            })
            .unwrap_or(0);
        buckets[bi].invoice_count += 1;
        buckets[bi].amount_cents += inv.balance_due_cents;

        if inv.days_overdue >= 61 {
            let d = debtors
                .entry(inv.customer_key.clone())
                .or_insert_with(|| AgingDebtor {
                    customer_key: inv.customer_key.clone(),
                    customer_name: inv.customer_name.clone(),
                    customer_phone: inv.customer_phone.clone(),
                    outstanding_cents: 0,
                    oldest_days: 0,
                    invoice_count: 0,
                });
            d.outstanding_cents += inv.balance_due_cents;
            d.oldest_days = d.oldest_days.max(inv.days_overdue);
            d.invoice_count += 1;
        }
    }

    let mut top_debtors: Vec<AgingDebtor> = debtors.into_values().collect();
    top_debtors.sort_by_key(|d| std::cmp::Reverse(d.outstanding_cents));
    top_debtors.truncate(10);

    Ok(ReceivablesAgingResponse {
        as_of,
        buckets,
        total_outstanding_cents,
        total_invoices: invoices.len() as u64,
        top_debtors,
    })
}

// ---------------------------------------------------------------------------
// discounts
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_discounts(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<DiscountAnalyticsResponse> {
    let (start, end) = range(&query)?;
    let agg = repository::analytics::aggregate_discounts(db, start, end).await?;

    let total_discount_cents = agg.order_level_discount_cents + agg.line_level_discount_cents;

    Ok(DiscountAnalyticsResponse {
        period_start: start,
        period_end: end,
        total_discount_cents,
        gross_before_discount_cents: agg.gross_before_discount_cents,
        discount_rate_bps: bps(total_discount_cents, agg.gross_before_discount_cents),
        invoices_with_discount: agg.invoices_with_discount,
        invoice_count: agg.invoice_count,
        order_level_discount_cents: agg.order_level_discount_cents,
        line_level_discount_cents: agg.line_level_discount_cents,
        by_type: agg
            .by_type
            .into_iter()
            .map(
                |(discount_type, invoice_count, discount_cents)| DiscountTypeRow {
                    discount_type,
                    invoice_count,
                    discount_cents,
                },
            )
            .collect(),
        by_cashier: {
            let mut v: Vec<DiscountCashierRow> = agg
                .by_cashier
                .into_iter()
                .map(
                    |(cashier_id, cashier_name, discount_cents, revenue_cents)| {
                        DiscountCashierRow {
                            discount_rate_bps: bps(discount_cents, revenue_cents + discount_cents),
                            cashier_id,
                            cashier_name,
                            discount_cents,
                            revenue_cents,
                        }
                    },
                )
                .collect();
            v.sort_by_key(|r| std::cmp::Reverse(r.discount_cents));
            v
        },
        top_discounted_products: agg
            .top_products
            .into_iter()
            .map(
                |(product_key, name, discount_cents, units_sold)| DiscountProductRow {
                    product_key,
                    name,
                    discount_cents,
                    units_sold,
                },
            )
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// refunds
// ---------------------------------------------------------------------------

pub(crate) async fn get_analytics_refunds(
    db: &Database,
    query: AnalyticsRangeQuery,
) -> AppResult<RefundAnalyticsResponse> {
    let (start, end) = range(&query)?;
    let agg = repository::analytics::aggregate_refund_detail(db, start, end).await?;
    let sales = repository::analytics::aggregate_invoice_kpis(db, start, end).await?;

    Ok(RefundAnalyticsResponse {
        period_start: start,
        period_end: end,
        credit_note_count: agg.credit_note_count,
        net_refund_cents: agg.net_refund_cents,
        refund_cash_cents: agg.refund_cash_cents,
        balance_reduction_cents: agg.balance_reduction_cents,
        refund_rate_bps: bps(agg.net_refund_cents, sales.net_sales_cents),
        exchange_count: agg.exchange_count,
        no_receipt_count: agg.no_receipt_count,
        manager_override_count: agg.manager_override_count,
        by_reason: agg
            .by_reason
            .into_iter()
            .map(
                |(reason, credit_note_item_count, amount_cents)| RefundReasonRow {
                    reason,
                    credit_note_item_count,
                    amount_cents,
                },
            )
            .collect(),
        by_method: agg
            .by_method
            .into_iter()
            .map(|(method, amount_cents)| RefundMethodRow {
                method,
                amount_cents,
            })
            .collect(),
        top_returned_products: agg
            .top_products
            .into_iter()
            .map(
                |(product_key, name, quantity, amount_cents)| RefundProductRow {
                    product_key,
                    name,
                    quantity,
                    amount_cents,
                },
            )
            .collect(),
    })
}
