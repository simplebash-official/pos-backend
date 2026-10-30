use super::super::sales::get_i64_flexible;
use super::refund_detail_agg::RefundDetailAgg;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;

use crate::core::error::AppResult;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

pub(crate) async fn aggregate_refund_detail(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<RefundDetailAgg> {
    match db {
        Db::Mongo(db) => {
            let item_total =
                doc! { "$ifNull": ["$returned_items.totalCents", "$returned_items.total_cents"] };
            let item_key =
                doc! { "$ifNull": ["$returned_items.productKey", "$returned_items.product_key"] };

            let pipeline = vec![
                doc! {
                    "$match": {
                        "status": { "$ne": "voided" },
                        "created_at": {
                            "$gte": BsonDateTime::from_chrono(start),
                            "$lt": BsonDateTime::from_chrono(end),
                        }
                    }
                },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "count": { "$sum": 1 },
                                    "net": { "$sum": "$net_refund_cents" },
                                    "cash": { "$sum": "$refund_cash_cents" },
                                    "balance": { "$sum": "$balance_reduction_cents" },
                                    "noReceipt": { "$sum": { "$cond": ["$no_receipt", 1, 0] } },
                                    "override": { "$sum": { "$cond": ["$is_manager_override", 1, 0] } },
                                    "exchange": {
                                        "$sum": {
                                            "$cond": [
                                                { "$gt": [{ "$size": { "$ifNull": ["$exchange_items", []] } }, 0] },
                                                1, 0
                                            ]
                                        }
                                    },
                                }
                            }
                        ],
                        "byMethod": [
                            { "$unwind": { "path": "$refund_breakdown", "preserveNullAndEmptyArrays": false } },
                            {
                                "$group": {
                                    "_id": "$refund_breakdown.method",
                                    "amount": { "$sum": "$refund_breakdown.amount_cents" },
                                }
                            }
                        ],
                        "byReason": [
                            { "$unwind": "$returned_items" },
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$returned_items.reason", "other"] },
                                    "count": { "$sum": 1 },
                                    "amount": { "$sum": item_total.clone() },
                                }
                            }
                        ],
                        "topProducts": [
                            { "$unwind": "$returned_items" },
                            {
                                "$group": {
                                    "_id": { "key": item_key.clone(), "name": "$returned_items.name" },
                                    "qty": { "$sum": { "$ifNull": ["$returned_items.quantity", 0] } },
                                    "amount": { "$sum": item_total.clone() },
                                }
                            },
                            { "$sort": { "amount": -1 } },
                            { "$limit": 10 },
                        ],
                    }
                },
            ];

            let mut cursor = db
                .collection::<Document>("credit_notes")
                .aggregate(pipeline)
                .await?;
            let mut agg = RefundDetailAgg::default();
            if let Some(result) = cursor.try_next().await? {
                if let Some(t) = result
                    .get_array("totals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.credit_note_count = get_i64_flexible(t, "count") as u64;
                    agg.net_refund_cents = get_i64_flexible(t, "net");
                    agg.refund_cash_cents = get_i64_flexible(t, "cash");
                    agg.balance_reduction_cents = get_i64_flexible(t, "balance");
                    agg.no_receipt_count = get_i64_flexible(t, "noReceipt") as u64;
                    agg.manager_override_count = get_i64_flexible(t, "override") as u64;
                    agg.exchange_count = get_i64_flexible(t, "exchange") as u64;
                }
                if let Ok(list) = result.get_array("byMethod") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_method.push((
                            row.get_str("_id").unwrap_or("cash").to_string(),
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("byReason") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_reason.push((
                            row.get_str("_id").unwrap_or("other").to_string(),
                            get_i64_flexible(row, "count") as u64,
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
                if let Ok(list) = result.get_array("topProducts") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let id = row.get_document("_id").ok();
                        agg.top_products.push((
                            id.and_then(|d| d.get_str("key").ok()).map(String::from),
                            id.and_then(|d| d.get_str("name").ok())
                                .unwrap_or("Unknown")
                                .to_string(),
                            get_i64_flexible(row, "qty"),
                            get_i64_flexible(row, "amount"),
                        ));
                    }
                }
            }
            Ok(agg)
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT * FROM credit_notes WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = RefundDetailAgg::default();
            let mut by_method_map: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            let mut by_reason_map: std::collections::HashMap<String, (u64, i64)> =
                std::collections::HashMap::new();
            let mut top_prods_map: std::collections::HashMap<(Option<String>, String), (i64, i64)> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                agg.credit_note_count += 1;
                agg.net_refund_cents += get_i64_flexible(&doc, "net_refund_cents");
                agg.refund_cash_cents += get_i64_flexible(&doc, "refund_cash_cents");
                agg.balance_reduction_cents += get_i64_flexible(&doc, "balance_reduction_cents");
                if doc.get_bool("no_receipt").unwrap_or(false) {
                    agg.no_receipt_count += 1;
                }
                if doc.get_bool("is_manager_override").unwrap_or(false) {
                    agg.manager_override_count += 1;
                }
                if let Ok(ex) = doc.get_array("exchange_items")
                    && !ex.is_empty()
                {
                    agg.exchange_count += 1;
                }

                if let Ok(bdown) = doc.get_array("refund_breakdown") {
                    for item in bdown.iter().filter_map(|i| i.as_document()) {
                        let m = item.get_str("method").unwrap_or("cash").to_string();
                        let amt = get_i64_flexible(item, "amount_cents");
                        *by_method_map.entry(m).or_insert(0) += amt;
                    }
                }

                if let Ok(ret_items) = doc.get_array("returned_items") {
                    for item in ret_items.iter().filter_map(|i| i.as_document()) {
                        let reason = item.get_str("reason").unwrap_or("other").to_string();
                        let amt = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let r_entry = by_reason_map.entry(reason).or_insert((0, 0));
                        r_entry.0 += 1;
                        r_entry.1 += amt;

                        let pk = item
                            .get_str("productKey")
                            .or_else(|_| item.get_str("product_key"))
                            .ok()
                            .map(String::from);
                        let name = item.get_str("name").unwrap_or("Unknown").to_string();
                        let qty = get_i64_flexible(item, "quantity");
                        let p_entry = top_prods_map.entry((pk, name)).or_insert((0, 0));
                        p_entry.0 += qty;
                        p_entry.1 += amt;
                    }
                }
            }

            agg.by_method = by_method_map.into_iter().collect();
            agg.by_reason = by_reason_map
                .into_iter()
                .map(|(r, (c, a))| (r, c, a))
                .collect();
            let mut top_prods: Vec<_> = top_prods_map
                .into_iter()
                .map(|((pk, name), (q, a))| (pk, name, q, a))
                .collect();
            top_prods.sort_by_key(|b| std::cmp::Reverse(b.3));
            top_prods.truncate(10);
            agg.top_products = top_prods;

            Ok(agg)
        }
    }
}
