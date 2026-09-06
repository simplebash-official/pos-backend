// Builds the `serde_json::Value` payload for the document-server
// `analytics-report` Typst template (`GET /reports/analytics/document`).
// Mirrors `billing::service::print_payload` in spirit: it does no rendering,
// only shaping — every series/table arrives pre-aggregated from the same
// `service::analytics` functions the JSON endpoints use, flattened into the
// parallel-array contract the `.typ` reads from `sys.inputs`.
//
// The shop letterhead (`shopName` / `shopAddressLines` / `logoUrl`) is *not*
// sent from here — it is stored once in the document-server's `template-data`
// for the `analytics-report` template and deep-merged in at render time.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::dates::{parse_date_range_tz, report_tz};
use crate::{
    clients::db::Db,
    core::error::AppResult,
    domain::reports::{
        AnalyticsKpiDeltas, AnalyticsKpis, AnalyticsRangeQuery, Granularity, ReceivablesAgingQuery,
        SalesByCategoryQuery, TopCustomersQuery, TopProductsQuery,
    },
    modules::reports::service,
};

/// `12345` -> `"Rs. 123.45"`, `-500000` -> `"-Rs. 5,000.00"`.
fn money(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let rupees = abs / 100;
    let paisa = abs % 100;
    // Group the integer part with thousands separators.
    let digits = rupees.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}Rs. {}.{:02}", if neg { "-" } else { "" }, grouped, paisa)
}

/// bps -> `"12.3%"`.
fn pct(bps: i64) -> String {
    format!("{}.{}%", bps / 100, (bps.abs() % 100) / 10)
}

fn delta_kpi(label: &str, value: String, delta_bps: Option<i64>) -> Value {
    match delta_bps {
        Some(b) => {
            let dir = if b > 50 {
                "up"
            } else if b < -50 {
                "down"
            } else {
                "flat"
            };
            json!({
                "label": label,
                "value": value,
                "deltaLabel": format!("{}{} vs previous", if b >= 0 { "+" } else { "-" }, pct(b)),
                "deltaDirection": dir,
            })
        }
        None => json!({ "label": label, "value": value }),
    }
}

fn kpi_cards(cur: &AnalyticsKpis, deltas: Option<&AnalyticsKpiDeltas>) -> Vec<Value> {
    let d = deltas;
    vec![
        delta_kpi(
            "Total revenue",
            money(cur.total_revenue_cents),
            d.and_then(|x| x.total_revenue_bps),
        ),
        delta_kpi(
            "Gross profit",
            money(cur.gross_profit_cents),
            d.and_then(|x| x.gross_profit_bps),
        ),
        delta_kpi(
            "Net profit",
            money(cur.net_profit_cents),
            d.and_then(|x| x.net_profit_bps),
        ),
        delta_kpi(
            "Invoices",
            cur.invoice_count.to_string(),
            d.and_then(|x| x.invoice_count_bps),
        ),
        delta_kpi(
            "Average basket",
            money(cur.avg_basket_cents),
            d.and_then(|x| x.avg_basket_bps),
        ),
        json!({ "label": "Gross margin", "value": pct(cur.gross_margin_bps) }),
        json!({ "label": "Discounts given", "value": money(cur.discount_cents) }),
        json!({ "label": "Refunds", "value": money(cur.refunds_cents) }),
    ]
}

fn granularity_from(
    q: &AnalyticsRangeQuery,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Granularity {
    match q.granularity.as_deref().map(str::to_lowercase).as_deref() {
        Some("day") => Granularity::Day,
        Some("week") => Granularity::Week,
        Some("month") => Granularity::Month,
        Some("year") => Granularity::Year,
        _ => {
            let days = (end - start).num_days();
            if days <= 31 {
                Granularity::Day
            } else if days <= 366 {
                Granularity::Week
            } else if days <= 1096 {
                Granularity::Month
            } else {
                Granularity::Year
            }
        }
    }
}

fn range_query(q: &AnalyticsRangeQuery) -> AnalyticsRangeQuery {
    AnalyticsRangeQuery {
        preset: q.preset.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
        granularity: q.granularity.clone(),
        compare_previous: Some(true),
    }
}

pub(crate) async fn build_analytics_report_data(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<Value> {
    let (start, end) = parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let gran = granularity_from(&query, start, end);
    let tz = report_tz();
    let start_local = start.with_timezone(&tz);
    // `end` is exclusive — show the last included day.
    let end_display = (end - chrono::Duration::seconds(1)).with_timezone(&tz);

    let period_label = format!(
        "{} – {}",
        start_local.format("%-d %b %Y"),
        end_display.format("%-d %b %Y")
    );

    // Fan out. Each of these runs the same code path as its JSON endpoint.
    let summary = service::get_analytics_summary(db, range_query(&query)).await?;
    let timeseries = service::get_analytics_timeseries(db, range_query(&query)).await?;
    let payments = service::get_analytics_payment_methods(db, range_query(&query)).await?;
    let by_category = service::get_analytics_sales_by_category(
        db,
        SalesByCategoryQuery {
            preset: query.preset.clone(),
            from: query.from.clone(),
            to: query.to.clone(),
            group_by: None,
        },
    )
    .await?;
    let top_customers = service::get_analytics_top_customers(
        db,
        TopCustomersQuery {
            preset: query.preset.clone(),
            from: query.from.clone(),
            to: query.to.clone(),
            limit: Some(10),
            sort_by: Some("revenue".to_string()),
        },
    )
    .await?;
    let cashiers = service::get_analytics_cashier_performance(db, range_query(&query)).await?;
    let patterns = service::get_analytics_sales_patterns(db, range_query(&query)).await?;
    let aging =
        service::get_analytics_receivables_aging(db, ReceivablesAgingQuery { as_of: None }).await?;
    let refunds = service::get_analytics_refunds(db, range_query(&query)).await?;
    let top_products = service::get_top_products(
        db,
        TopProductsQuery {
            preset: query.preset.clone(),
            from: query.from.clone(),
            to: query.to.clone(),
            limit: Some(10),
            sort_by: Some("revenue".to_string()),
        },
    )
    .await?;
    let inventory = service::get_inventory_valuation(db).await?;

    let ts = &timeseries.points;
    let comparison_label = if summary.previous.is_some() {
        let span_days = (end - start).num_days();
        format!("vs previous {span_days} days")
    } else {
        String::new()
    };

    Ok(json!({
        "generatedAt": Utc::now().format("%-d %b %Y, %H:%M").to_string(),
        "periodLabel": period_label,
        "granularityLabel": gran.label(),
        "comparisonLabel": comparison_label,
        "currency": "LKR",
        "kpis": kpi_cards(&summary.current, summary.deltas.as_ref()),
        "timeseries": {
            "labels": ts.iter().map(|p| p.label.clone()).collect::<Vec<_>>(),
            "revenueCents": ts.iter().map(|p| p.revenue_cents).collect::<Vec<_>>(),
            "retailRevenueCents": ts.iter().map(|p| p.retail_revenue_cents).collect::<Vec<_>>(),
            "repairRevenueCents": ts.iter().map(|p| p.repair_revenue_cents).collect::<Vec<_>>(),
            "printRevenueCents": ts.iter().map(|p| p.print_revenue_cents).collect::<Vec<_>>(),
            "cogsCents": ts.iter().map(|p| p.cogs_cents).collect::<Vec<_>>(),
            "grossProfitCents": ts.iter().map(|p| p.gross_profit_cents).collect::<Vec<_>>(),
            "grossMarginBps": ts.iter().map(|p| p.gross_margin_bps).collect::<Vec<_>>(),
        },
        "paymentMethods": [
            { "label": "Cash", "amountCents": payments.breakdown.cash_cents },
            { "label": "Card", "amountCents": payments.breakdown.card_cents },
            { "label": "Online", "amountCents": payments.breakdown.online_cents },
            { "label": "Credit", "amountCents": payments.breakdown.credit_cents },
        ],
        "salesByCategory": by_category.rows.iter().take(12).map(|r| json!({
            "name": r.category_name,
            "unitsSold": r.units_sold,
            "revenueCents": r.revenue_cents,
            "grossProfitCents": r.gross_profit_cents,
        })).collect::<Vec<_>>(),
        "topProducts": top_products.products.iter().map(|p| json!({
            "name": p.name,
            "unitsSold": p.units_sold,
            "revenueCents": p.total_revenue_cents,
        })).collect::<Vec<_>>(),
        "topCustomers": top_customers.customers.iter().map(|c| json!({
            "name": c.customer_name,
            "invoiceCount": c.invoice_count,
            "revenueCents": c.revenue_cents,
            "grossProfitCents": c.gross_profit_cents,
            "outstandingCents": c.outstanding_cents,
        })).collect::<Vec<_>>(),
        "cashierPerformance": cashiers.cashiers.iter().map(|c| json!({
            "name": c.cashier_name,
            "invoiceCount": c.invoice_count,
            "revenueCents": c.revenue_cents,
            "discountGivenCents": c.discount_given_cents,
            "grossProfitCents": c.gross_profit_cents,
        })).collect::<Vec<_>>(),
        "receivablesAging": aging.buckets.iter().map(|b| json!({
            "label": b.label,
            "amountCents": b.amount_cents,
            "invoiceCount": b.invoice_count,
        })).collect::<Vec<_>>(),
        "weekdayPattern": patterns.by_weekday.iter().map(|b| b.revenue_cents).collect::<Vec<_>>(),
        "refunds": {
            "creditNoteCount": refunds.credit_note_count,
            "netRefundCents": refunds.net_refund_cents,
            "refundCashCents": refunds.refund_cash_cents,
            "byReason": refunds.by_reason.iter().map(|r| json!({
                "reason": r.reason, "amountCents": r.amount_cents, "count": r.credit_note_item_count,
            })).collect::<Vec<_>>(),
        },
        "inventoryValuation": {
            "totalCostValuationCents": inventory.total_cost_valuation_cents,
            "totalRetailValuationCents": inventory.total_retail_valuation_cents,
            "potentialGrossProfitCents": inventory.potential_gross_profit_cents,
            "lowStockProductsCount": inventory.low_stock_products_count,
            "outOfStockProductsCount": inventory.out_of_stock_products_count,
            "categories": inventory.categories.iter().take(12).map(|c| json!({
                "name": c.category_name,
                "costValuationCents": c.cost_valuation_cents,
                "retailValuationCents": c.retail_valuation_cents,
            })).collect::<Vec<_>>(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::{money, pct};

    #[test]
    fn money_formats_lkr_with_thousands_and_paisa() {
        assert_eq!(money(0), "Rs. 0.00");
        assert_eq!(money(100), "Rs. 1.00");
        assert_eq!(money(12_345), "Rs. 123.45");
        assert_eq!(money(1_234_567), "Rs. 12,345.67");
        assert_eq!(money(-500_000), "-Rs. 5,000.00");
    }

    #[test]
    fn pct_formats_bps_as_one_decimal_percent() {
        assert_eq!(pct(1_234), "12.3%");
        assert_eq!(pct(0), "0.0%");
        assert_eq!(pct(10_000), "100.0%");
    }
}
