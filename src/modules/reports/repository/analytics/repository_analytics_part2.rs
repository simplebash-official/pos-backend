use super::super::super::service::dates::REPORT_TZ_MONGO;
use super::super::sales::get_i64_flexible;
use super::repository_analytics_part3::{
    CategoryAggRow, RawSeriesBucket, RefundDetailAgg, TopCustomerAggRow, date_match, invoices,
    trunc,
};
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::core::error::AppResult;
use crate::domain::reports::Granularity;
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

/// Revenue / retail-COGS / discount / invoice-count per `$dateTrunc` bucket.
/// Sparse — empty buckets are absent and the service layer gap-fills them.
pub(crate) async fn aggregate_revenue_series(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    granularity: Granularity,
) -> AppResult<Vec<RawSeriesBucket>> {
    match db {
        Db::Mongo(db) => {
            let unit = granularity.as_mongo_unit();
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! { "$addFields": { "bucket": trunc(unit) } },
                doc! {
                    "$facet": {
                        "invoiceBuckets": [
                            {
                                "$group": {
                                    "_id": "$bucket",
                                    "count": { "$sum": 1 },
                                    "discount": { "$sum": "$discount_cents" },
                                }
                            }
                        ],
                        "streamBuckets": [
                            { "$unwind": "$items" },
                            {
                                "$group": {
                                    "_id": { "bucket": "$bucket", "sourceType": source_type.clone() },
                                    "revenue": { "$sum": line_total.clone() },
                                    "cogs": {
                                        "$sum": {
                                            "$multiply": [
                                                { "$ifNull": [unit_cost.clone(), 0] },
                                                { "$ifNull": ["$items.quantity", 0] },
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
            let mut map: std::collections::BTreeMap<i64, RawSeriesBucket> =
                std::collections::BTreeMap::new();

            let bucket_instant = |doc: &Document, key: &str| -> Option<DateTime<Utc>> {
                doc.get_datetime(key).ok().map(|d| d.to_chrono())
            };

            if let Some(result) = cursor.try_next().await? {
                if let Ok(rows) = result.get_array("invoiceBuckets") {
                    for row in rows.iter().filter_map(|r| r.as_document()) {
                        let Some(ts) = bucket_instant(row, "_id") else {
                            continue;
                        };
                        let entry =
                            map.entry(ts.timestamp_millis())
                                .or_insert_with(|| RawSeriesBucket {
                                    period_start: ts,
                                    ..Default::default()
                                });
                        entry.invoice_count += get_i64_flexible(row, "count") as u64;
                        entry.discount_cents += get_i64_flexible(row, "discount");
                    }
                }

                if let Ok(rows) = result.get_array("streamBuckets") {
                    for row in rows.iter().filter_map(|r| r.as_document()) {
                        let Ok(id) = row.get_document("_id") else {
                            continue;
                        };
                        let Some(ts) = bucket_instant(id, "bucket") else {
                            continue;
                        };
                        let revenue = get_i64_flexible(row, "revenue");
                        let cogs = get_i64_flexible(row, "cogs");
                        let entry =
                            map.entry(ts.timestamp_millis())
                                .or_insert_with(|| RawSeriesBucket {
                                    period_start: ts,
                                    ..Default::default()
                                });
                        entry.cogs_cents += cogs;
                        match id.get_str("sourceType").unwrap_or("") {
                            "retail" => entry.retail_revenue_cents += revenue,
                            "repair" => entry.repair_revenue_cents += revenue,
                            "print" => entry.print_revenue_cents += revenue,
                            _ => {}
                        }
                    }
                }
            }

            Ok(map.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT created_at, discount_cents, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::BTreeMap<DateTime<Utc>, RawSeriesBucket> =
                std::collections::BTreeMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono()
                } else {
                    continue;
                };
                let bucket_start = align_bucket_instant(created_at, granularity);
                let entry = map.entry(bucket_start).or_insert_with(|| RawSeriesBucket {
                    period_start: bucket_start,
                    ..Default::default()
                });
                entry.invoice_count += 1;
                entry.discount_cents += get_i64_flexible(&doc, "discount_cents");

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
                                entry.retail_revenue_cents += line_total;
                                if let Some(cost) = unit_cost {
                                    entry.cogs_cents += cost * qty;
                                }
                            }
                            "repair" => entry.repair_revenue_cents += line_total,
                            "print" => entry.print_revenue_cents += line_total,
                            _ => {}
                        }
                    }
                }
            }

            Ok(map.into_values().collect())
        }
    }
}

/// Retail sales rolled up by product category (or subcategory). A retail line
/// whose product no longer resolves is returned with `category_key = None`.
pub(crate) async fn aggregate_sales_by_category(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    by_subcategory: bool,
) -> AppResult<Vec<CategoryAggRow>> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let line_discount =
                doc! { "$ifNull": ["$items.discountCents", "$items.discount_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };
            let product_key = doc! { "$ifNull": ["$items.productKey", "$items.product_key"] };

            let group_id = if by_subcategory {
                doc! {
                    "categoryKey": "$prod.category_key",
                    "categoryName": "$cat.name",
                    "subcategoryKey": "$prod.subcategory_key",
                    "subcategoryName": "$subcat.name",
                }
            } else {
                doc! {
                    "categoryKey": "$prod.category_key",
                    "categoryName": "$cat.name",
                }
            };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! { "$unwind": "$items" },
                doc! { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                doc! { "$addFields": { "pk": product_key.clone() } },
                doc! {
                    "$lookup": {
                        "from": "products",
                        "localField": "pk",
                        "foreignField": "key",
                        "as": "prod",
                    }
                },
                doc! { "$addFields": { "prod": { "$arrayElemAt": ["$prod", 0] } } },
                doc! {
                    "$lookup": {
                        "from": "categories",
                        "localField": "prod.category_key",
                        "foreignField": "key",
                        "as": "cat",
                    }
                },
                doc! { "$addFields": { "cat": { "$arrayElemAt": ["$cat", 0] } } },
                doc! {
                    "$lookup": {
                        "from": "subcategories",
                        "localField": "prod.subcategory_key",
                        "foreignField": "key",
                        "as": "subcat",
                    }
                },
                doc! { "$addFields": { "subcat": { "$arrayElemAt": ["$subcat", 0] } } },
                doc! {
                    "$group": {
                        "_id": group_id,
                        "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                        "revenue": { "$sum": line_total.clone() },
                        "discount": { "$sum": line_discount.clone() },
                        "cogs": {
                            "$sum": {
                                "$multiply": [
                                    { "$ifNull": [unit_cost.clone(), 0] },
                                    { "$ifNull": ["$items.quantity", 0] },
                                ]
                            }
                        },
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut rows = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let id = row.get_document("_id").ok();
                rows.push(CategoryAggRow {
                    category_key: id
                        .and_then(|d| d.get_str("categoryKey").ok())
                        .map(String::from),
                    category_name: id
                        .and_then(|d| d.get_str("categoryName").ok())
                        .unwrap_or("Uncategorised")
                        .to_string(),
                    subcategory_key: id
                        .and_then(|d| d.get_str("subcategoryKey").ok())
                        .map(String::from),
                    subcategory_name: id
                        .and_then(|d| d.get_str("subcategoryName").ok())
                        .map(String::from),
                    units_sold: get_i64_flexible(&row, "units"),
                    revenue_cents: get_i64_flexible(&row, "revenue"),
                    discount_cents: get_i64_flexible(&row, "discount"),
                    cogs_cents: get_i64_flexible(&row, "cogs"),
                });
            }
            Ok(rows)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();

            let prod_rows = sqlx::query("SELECT key, category_key, subcategory_key FROM products")
                .fetch_all(pool)
                .await?;
            let mut prod_map: std::collections::HashMap<String, (String, Option<String>)> =
                std::collections::HashMap::new();
            for r in prod_rows {
                let k: String = r.get("key");
                let cat_k: String = r.get("category_key");
                let subcat_k: Option<String> = r.get("subcategory_key");
                prod_map.insert(k, (cat_k, subcat_k));
            }

            let cat_rows = sqlx::query("SELECT key, name FROM categories")
                .fetch_all(pool)
                .await?;
            let mut cat_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for r in cat_rows {
                cat_map.insert(r.get("key"), r.get("name"));
            }

            let subcat_rows = sqlx::query("SELECT key, name FROM subcategories")
                .fetch_all(pool)
                .await?;
            let mut subcat_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for r in subcat_rows {
                subcat_map.insert(r.get("key"), r.get("name"));
            }

            let invoice_rows = sqlx::query(
                "SELECT items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            type GroupKey = (Option<String>, String, Option<String>, Option<String>);
            let mut groups: std::collections::HashMap<GroupKey, CategoryAggRow> =
                std::collections::HashMap::new();

            for row in &invoice_rows {
                let doc = map_sqlite_row_to_document(row);
                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        if source_type != "retail" {
                            continue;
                        }
                        let pk = item
                            .get_str("productKey")
                            .or_else(|_| item.get_str("product_key"))
                            .unwrap_or("");
                        let (cat_key, cat_name, subcat_key, subcat_name) =
                            if let Some((ck, sk)) = prod_map.get(pk) {
                                let cname = cat_map
                                    .get(ck)
                                    .cloned()
                                    .unwrap_or_else(|| "Uncategorised".to_string());
                                let sname = sk.as_ref().and_then(|k| subcat_map.get(k).cloned());
                                (Some(ck.clone()), cname, sk.clone(), sname)
                            } else {
                                (None, "Uncategorised".to_string(), None, None)
                            };

                        let gkey = if by_subcategory {
                            (
                                cat_key.clone(),
                                cat_name.clone(),
                                subcat_key.clone(),
                                subcat_name.clone(),
                            )
                        } else {
                            (cat_key.clone(), cat_name.clone(), None, None)
                        };

                        let line_total = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")
                        } else {
                            get_i64_flexible(item, "total_cents")
                        };
                        let line_discount = if item.contains_key("discountCents") {
                            get_i64_flexible(item, "discountCents")
                        } else {
                            get_i64_flexible(item, "discount_cents")
                        };
                        let unit_cost = if item.contains_key("unitCostCents") {
                            get_i64_flexible(item, "unitCostCents")
                        } else {
                            get_i64_flexible(item, "unit_cost_cents")
                        };
                        let qty = get_i64_flexible(item, "quantity");

                        let entry = groups.entry(gkey).or_insert_with(|| CategoryAggRow {
                            category_key: cat_key,
                            category_name: cat_name,
                            subcategory_key: if by_subcategory { subcat_key } else { None },
                            subcategory_name: if by_subcategory { subcat_name } else { None },
                            ..Default::default()
                        });
                        entry.units_sold += qty;
                        entry.revenue_cents += line_total;
                        entry.discount_cents += line_discount;
                        entry.cogs_cents += unit_cost * qty;
                    }
                }
            }
            Ok(groups.into_values().collect())
        }
    }
}

// ---------------------------------------------------------------------------
// sales patterns (weekday x hour)
// ---------------------------------------------------------------------------

/// `(weekday 1..7, hour 0..23, invoice count, revenue)` cells present in the
/// data — the service layer zero-fills the full 7×24 grid.
pub(crate) async fn aggregate_sales_patterns(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<(u8, u8, u64, i64)>> {
    match db {
        Db::Mongo(db) => {
            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$group": {
                        "_id": {
                            "weekday": { "$isoDayOfWeek": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                            "hour": { "$hour": { "date": "$created_at", "timezone": REPORT_TZ_MONGO } },
                        },
                        "count": { "$sum": 1 },
                        "revenue": { "$sum": "$total_cents" },
                    }
                },
            ];
            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut out = Vec::new();
            while let Some(row) = cursor.try_next().await? {
                let Ok(id) = row.get_document("_id") else {
                    continue;
                };
                let weekday = get_i64_flexible(id, "weekday").clamp(1, 7) as u8;
                let hour = get_i64_flexible(id, "hour").clamp(0, 23) as u8;
                out.push((
                    weekday,
                    hour,
                    get_i64_flexible(&row, "count") as u64,
                    get_i64_flexible(&row, "revenue"),
                ));
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use chrono::{Datelike, FixedOffset, Timelike};
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT created_at, total_cents FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("+05:30 is a valid offset");
            let mut map: std::collections::HashMap<(u8, u8), (u64, i64)> =
                std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let created_at = if let Ok(bdt) = doc.get_datetime("created_at") {
                    bdt.to_chrono()
                } else {
                    continue;
                };
                let local = created_at.with_timezone(&tz);
                let weekday = local.weekday().number_from_monday() as u8;
                let hour = local.hour() as u8;
                let rev = get_i64_flexible(&doc, "total_cents");
                let entry = map.entry((weekday, hour)).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += rev;
            }

            let out = map
                .into_iter()
                .map(|((w, h), (cnt, rev))| (w, h, cnt, rev))
                .collect();
            Ok(out)
        }
    }
}

/// Per-customer sales over `[start, end)`. Walk-in invoices (no `customer_key`)
/// are returned as one row with `customer_key = None`.
pub(crate) async fn aggregate_top_customers(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<TopCustomerAggRow>> {
    match db {
        Db::Mongo(db) => {
            let source_type = doc! { "$ifNull": ["$items.sourceType", "$items.source_type"] };
            let line_total = doc! { "$ifNull": ["$items.totalCents", "$items.total_cents"] };
            let unit_cost = doc! { "$ifNull": ["$items.unitCostCents", "$items.unit_cost_cents"] };
            let cust = doc! { "$ifNull": ["$customer_key", null] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "byCustomer": [
                            {
                                "$group": {
                                    "_id": {
                                        "key": cust.clone(),
                                        "name": { "$ifNull": ["$customer_name_snapshot", "Walk-in"] },
                                        "phone": "$customer_phone_snapshot",
                                    },
                                    "count": { "$sum": 1 },
                                    "revenue": { "$sum": "$total_cents" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "last": { "$max": "$created_at" },
                                }
                            }
                        ],
                        "retailByCustomer": [
                            { "$addFields": { "custKey": cust.clone() } },
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$eq": [source_type.clone(), "retail"] } } },
                            {
                                "$group": {
                                    "_id": "$custKey",
                                    "retailRevenue": { "$sum": line_total.clone() },
                                    "cogs": {
                                        "$sum": {
                                            "$multiply": [
                                                { "$ifNull": [unit_cost.clone(), 0] },
                                                { "$ifNull": ["$items.quantity", 0] },
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
            let mut rows: std::collections::HashMap<Option<String>, TopCustomerAggRow> =
                std::collections::HashMap::new();

            if let Some(result) = cursor.try_next().await? {
                if let Ok(list) = result.get_array("byCustomer") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let Ok(id) = row.get_document("_id") else {
                            continue;
                        };
                        let key = id.get_str("key").ok().map(String::from);
                        let entry = rows.entry(key.clone()).or_default();
                        if entry.customer_name.is_empty() {
                            entry.customer_key = key;
                            entry.customer_name =
                                id.get_str("name").unwrap_or("Walk-in").to_string();
                            entry.customer_phone = id.get_str("phone").ok().map(String::from);
                        }
                        entry.invoice_count += get_i64_flexible(row, "count") as u64;
                        entry.revenue_cents += get_i64_flexible(row, "revenue");
                        entry.discount_cents += get_i64_flexible(row, "discount");
                        if let Ok(dt) = row.get_datetime("last") {
                            let ts = dt.to_chrono();
                            if ts > entry.last_purchase_at {
                                entry.last_purchase_at = ts;
                            }
                        }
                    }
                }
                if let Ok(list) = result.get_array("retailByCustomer") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let key = row.get_str("_id").ok().map(String::from);
                        let profit =
                            get_i64_flexible(row, "retailRevenue") - get_i64_flexible(row, "cogs");
                        rows.entry(key).or_default().gross_profit_cents += profit;
                    }
                }
            }

            Ok(rows.into_values().collect())
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT customer_key, customer_name_snapshot, customer_phone_snapshot, total_cents, discount_cents, created_at, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut map: std::collections::HashMap<Option<String>, TopCustomerAggRow> =
                std::collections::HashMap::new();
            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let key = doc
                    .get_str("customer_key")
                    .ok()
                    .map(String::from)
                    .filter(|s| !s.is_empty());
                let entry = map.entry(key.clone()).or_default();
                if entry.customer_name.is_empty() {
                    entry.customer_key = key;
                    entry.customer_name = doc
                        .get_str("customer_name_snapshot")
                        .unwrap_or("Walk-in")
                        .to_string();
                    entry.customer_phone = doc
                        .get_str("customer_phone_snapshot")
                        .ok()
                        .map(String::from);
                }
                entry.invoice_count += 1;
                entry.revenue_cents += get_i64_flexible(&doc, "total_cents");
                entry.discount_cents += get_i64_flexible(&doc, "discount_cents");
                if let Ok(bdt) = doc.get_datetime("created_at") {
                    let ts = bdt.to_chrono();
                    if ts > entry.last_purchase_at {
                        entry.last_purchase_at = ts;
                    }
                }

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let source_type = item
                            .get_str("sourceType")
                            .or_else(|_| item.get_str("source_type"))
                            .unwrap_or("");
                        if source_type == "retail" {
                            let line_total = if item.contains_key("totalCents") {
                                get_i64_flexible(item, "totalCents")
                            } else {
                                get_i64_flexible(item, "total_cents")
                            };
                            let unit_cost = if item.contains_key("unitCostCents") {
                                get_i64_flexible(item, "unitCostCents")
                            } else {
                                get_i64_flexible(item, "unit_cost_cents")
                            };
                            let qty = get_i64_flexible(item, "quantity");
                            let cogs = unit_cost * qty;
                            entry.gross_profit_cents += line_total - cogs;
                        }
                    }
                }
            }
            Ok(map.into_values().collect())
        }
    }
}

// ---------------------------------------------------------------------------
// receivables aging
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct AgingInvoiceRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub balance_due_cents: i64,
    pub days_overdue: i32,
}

pub(super) fn align_bucket_instant(instant: DateTime<Utc>, gran: Granularity) -> DateTime<Utc> {
    use chrono::{Datelike, Duration, FixedOffset, NaiveDate};
    let tz = FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("+05:30 is a valid offset");
    let local = instant.with_timezone(&tz);
    let date = local.date_naive();
    let aligned_date = match gran {
        Granularity::Day => date,
        Granularity::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
        Granularity::Month => NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .expect("the first of a month is a valid date"),
        Granularity::Year => NaiveDate::from_ymd_opt(date.year(), 1, 1)
            .expect("the first of a month is a valid date"),
    };
    aligned_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is a valid time")
        .and_local_timezone(tz)
        .single()
        .expect("a fixed offset has no ambiguous local time")
        .with_timezone(&Utc)
}
