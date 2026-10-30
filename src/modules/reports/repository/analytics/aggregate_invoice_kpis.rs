use super::super::sales::get_i64_flexible;
use super::date_match::date_match;
use super::invoice_kpi_aggregate::InvoiceKpiAggregate;
use super::invoices::invoices;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Invoice-side KPIs for `[start, end)`: totals, per-stream revenue, retail
/// COGS from the per-line cost snapshot, and the coverage numerator.
pub(crate) async fn aggregate_invoice_kpis(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<InvoiceKpiAggregate> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "gross": { "$sum": "$subtotal_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "net": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "streams": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": source_type.clone(),
                                    "revenue": { "$sum": line_total.clone() },
                                }
                            }
                        ],
                        "retail": [
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                            {
                                "$group": {
                                    "_id": null,
                                    "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                                    "cogs": {
                                        "$sum": {
                                            "$multiply": [
                                                { "$ifNull": [unit_cost.clone(), 0] },
                                                { "$ifNull": ["$items.quantity", 0] },
                                            ]
                                        }
                                    },
                                    "revenueWithCost": {
                                        "$sum": {
                                            "$cond": [
                                                { "$ne": [unit_cost.clone(), null] },
                                                line_total.clone(),
                                                0,
                                            ]
                                        }
                                    },
                                }
                            }
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut agg = InvoiceKpiAggregate::default();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.invoice_count = get_i64_flexible(first, "count") as u64;
                    agg.gross_sales_cents = get_i64_flexible(first, "gross");
                    agg.discount_cents = get_i64_flexible(first, "discount");
                    agg.net_sales_cents = get_i64_flexible(first, "net");
                }

                if let Ok(streams) = result.get_array("streams") {
                    for stream in streams.iter().filter_map(|s| s.as_document()) {
                        let revenue = get_i64_flexible(stream, "revenue");
                        match stream.get_str("_id").unwrap_or("") {
                            "retail" => agg.retail_revenue_cents += revenue,
                            "repair" => agg.repair_revenue_cents += revenue,
                            "print" => agg.print_revenue_cents += revenue,
                            _ => {}
                        }
                    }
                }

                if let Ok(retail) = result.get_array("retail")
                    && let Some(first) = retail.first().and_then(|v| v.as_document())
                {
                    agg.retail_items_sold = get_i64_flexible(first, "units");
                    agg.retail_cogs_cents = get_i64_flexible(first, "cogs");
                    agg.retail_revenue_with_cost_cents = get_i64_flexible(first, "revenueWithCost");
                }
            }

            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT subtotal_cents, discount_cents, total_cents, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = InvoiceKpiAggregate::default();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.invoice_count += 1;
                agg.gross_sales_cents += get_i64_flexible(&doc, "subtotal_cents");
                agg.discount_cents += get_i64_flexible(&doc, "discount_cents");
                agg.net_sales_cents += get_i64_flexible(&doc, "total_cents");

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            item.get_i64("unitCostCents").ok()
                        } else {
                            item.get_i64("unit_cost_cents").ok()
                        };
                        let qty = get_i64_flexible(item, "quantity");

                        match source_type {
                            "retail" => {
                                agg.retail_revenue_cents += line_total;
                                agg.retail_items_sold += qty;
                                if let Some(cost) = unit_cost {
                                    agg.retail_cogs_cents += cost * qty;
                                    agg.retail_revenue_with_cost_cents += line_total;
                                }
                            }
                            "repair" => agg.repair_revenue_cents += line_total,
                            "print" => agg.print_revenue_cents += line_total,
                            _ => {}
                        }
                    }
                }
            }
            Ok(agg)
        }
    }
}
