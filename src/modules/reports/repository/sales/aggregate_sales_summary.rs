use super::compute_payment_methods::compute_payment_methods;
use super::get_i64_flexible::get_i64_flexible;
use super::invoices::invoices;
use super::sales_summary_aggregate::SalesSummaryAggregate;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Runs a `$facet` aggregation over `invoices` within `[start, end)` to compute
/// overall totals, stream breakdowns (retail, repair, print), and payment methods.
pub(crate) async fn aggregate_sales_summary(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<SalesSummaryAggregate> {
    match db {
        Db::Mongo(db) => {
            let match_stage = doc! {
                "$match": {
                    "status": { "$ne": "voided" },
                    "created_at": {
                        "$gte": BsonDateTime::from_chrono(start),
                        "$lt": BsonDateTime::from_chrono(end),
                    }
                }
            };

            let pipeline = vec![
                match_stage.clone(),
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "subtotal": { "$sum": "$subtotal_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "total": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "streams": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$items.sourceType", "$items.source_type"] },
                                    "revenue": { "$sum": { "$ifNull": ["$items.totalCents", "$items.total_cents"] } },
                                }
                            }
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut agg = SalesSummaryAggregate::default();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(totals) = result.get_array("totals")
                    && let Some(first) = totals.first().and_then(|v| v.as_document())
                {
                    agg.total_invoices = get_i64_flexible(first, "count") as u64;
                    agg.gross_sales_cents = get_i64_flexible(first, "subtotal");
                    agg.discount_cents = get_i64_flexible(first, "discount");
                    agg.total_sales_cents = get_i64_flexible(first, "total");
                }

                if let Ok(streams) = result.get_array("streams") {
                    for stream in streams {
                        if let Some(doc) = stream.as_document() {
                            let source_type = doc.get_str("_id").unwrap_or("");
                            let revenue = get_i64_flexible(doc, "revenue");
                            match source_type {
                                "retail" => agg.retail_revenue_cents += revenue,
                                "repair" => agg.repair_revenue_cents += revenue,
                                "print" => agg.print_revenue_cents += revenue,
                                _ => {}
                            }
                        }
                    }
                }
            }

            agg.payment_methods =
                compute_payment_methods(&Db::Mongo(db.clone()), start, end).await?;
            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT * FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = SalesSummaryAggregate::default();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.total_invoices += 1;
                agg.gross_sales_cents += get_i64_flexible(&doc, "subtotal_cents");
                agg.discount_cents += get_i64_flexible(&doc, "discount_cents");
                agg.total_sales_cents += get_i64_flexible(&doc, "total_cents");

                if let Ok(items) = doc.get_array("items") {
                    for item in items {
                        if let Some(idoc) = item.as_document() {
                            let source_type = idoc
                                .get_str("sourceType")
                                .or_else(|_| idoc.get_str("source_type"))
                                .unwrap_or("");
                            let revenue = if idoc.contains_key("totalCents") {
                                get_i64_flexible(idoc, "totalCents")
                            } else {
                                get_i64_flexible(idoc, "total_cents")
                            };
                            match source_type {
                                "retail" => agg.retail_revenue_cents += revenue,
                                "repair" => agg.repair_revenue_cents += revenue,
                                "print" => agg.print_revenue_cents += revenue,
                                _ => {}
                            }
                        }
                    }
                }
            }

            agg.payment_methods = compute_payment_methods(db, start, end).await?;
            Ok(agg)
        }
    }
}
