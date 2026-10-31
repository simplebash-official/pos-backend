//! Refund detail, cashier refunds and discount analytics.

use super::super::sales::{get_i64_flexible, required_text};
use super::analytics_shared::{date_match, invoices};

use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::core::error::AppResult;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// `cashier_id -> (refund credit-note count, Σ net_refund_cents)` over
/// non-voided credit notes in `[start, end)`.
pub(crate) async fn aggregate_cashier_refunds(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<std::collections::HashMap<String, (u64, i64)>> {
    match db {
        Db::Mongo(db) => {
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
                    "$group": {
                        "_id": "$cashier_id",
                        "count": { "$sum": 1 },
                        "amount": { "$sum": "$net_refund_cents" },
                    }
                },
            ];
            let mut cursor = db
                .collection::<Document>("credit_notes")
                .aggregate(pipeline)
                .await?;
            let mut out = std::collections::HashMap::new();
            while let Some(row) = cursor.try_next().await? {
                if let Ok(cid) = row.get_str("_id") {
                    out.insert(
                        cid.to_string(),
                        (
                            get_i64_flexible(&row, "count")? as u64,
                            get_i64_flexible(&row, "amount")?,
                        ),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            use sqlx::Row;
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();
            let rows = sqlx::query(
                "SELECT cashier_id, COUNT(*) as cnt, COALESCE(SUM(net_refund_cents), 0) as amount FROM credit_notes WHERE status != 'voided' AND created_at >= ? AND created_at < ? GROUP BY cashier_id"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut out = std::collections::HashMap::new();
            for r in rows {
                let cid: Option<String> = r.get("cashier_id");
                if let Some(cid) = cid {
                    let cnt: i64 = r.get("cnt");
                    let amt: i64 = r.get("amount");
                    out.insert(cid, (cnt as u64, amt));
                }
            }
            Ok(out)
        }
    }
}

pub(crate) async fn aggregate_discounts(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<DiscountAgg> {
    match db {
        Db::Mongo(db) => {
            let line_discount =
                doc! { "$ifNull": ["$items.discountCents", "$items.discount_cents"] };
            let product_key = doc! { "$ifNull": ["$items.productKey", "$items.product_key"] };

            let pipeline = vec![
                doc! { "$match": date_match(start, end) },
                doc! {
                    "$facet": {
                        "totals": [
                            {
                                "$group": {
                                    "_id": null,
                                    "gross": { "$sum": "$subtotal_cents" },
                                    "orderDiscount": { "$sum": "$discount_cents" },
                                    "count": { "$sum": 1 },
                                    "withDiscount": {
                                        "$sum": { "$cond": [{ "$gt": ["$discount_cents", 0] }, 1, 0] }
                                    },
                                }
                            }
                        ],
                        "byType": [
                            {
                                "$group": {
                                    "_id": { "$ifNull": ["$discount_type", "none"] },
                                    "count": { "$sum": 1 },
                                    "discount": { "$sum": "$discount_cents" },
                                }
                            }
                        ],
                        "byCashier": [
                            {
                                "$group": {
                                    "_id": { "id": "$cashier_id", "name": "$cashier_name_snapshot" },
                                    "discount": { "$sum": "$discount_cents" },
                                    "revenue": { "$sum": "$total_cents" },
                                }
                            }
                        ],
                        "lineTotals": [
                            { "$unwind": "$items" },
                            { "$group": { "_id": null, "lineDiscount": { "$sum": line_discount.clone() } } }
                        ],
                        "topProducts": [
                            { "$unwind": "$items" },
                            { "$match": { "$expr": { "$gt": [line_discount.clone(), 0] } } },
                            {
                                "$group": {
                                    "_id": { "key": product_key.clone(), "name": "$items.name" },
                                    "discount": { "$sum": line_discount.clone() },
                                    "units": { "$sum": { "$ifNull": ["$items.quantity", 0] } },
                                }
                            },
                            { "$sort": { "discount": -1 } },
                            { "$limit": 10 },
                        ],
                    }
                },
            ];

            let mut cursor = invoices(db).aggregate(pipeline).await?;
            let mut agg = DiscountAgg::default();
            if let Some(result) = cursor.try_next().await? {
                if let Some(t) = result
                    .get_array("totals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.gross_before_discount_cents = get_i64_flexible(t, "gross")?;
                    agg.order_level_discount_cents = get_i64_flexible(t, "orderDiscount")?;
                    agg.invoice_count = get_i64_flexible(t, "count")? as u64;
                    agg.invoices_with_discount = get_i64_flexible(t, "withDiscount")? as u64;
                }
                if let Some(l) = result
                    .get_array("lineTotals")
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_document())
                {
                    agg.line_level_discount_cents = get_i64_flexible(l, "lineDiscount")?;
                }
                if let Ok(list) = result.get_array("byType") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_type.push((
                            row.get_str("_id").unwrap_or("none").to_string(),
                            get_i64_flexible(row, "count")? as u64,
                            get_i64_flexible(row, "discount")?,
                        ));
                    }
                }
                if let Ok(list) = result.get_array("byCashier") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        let id = row.get_document("_id").ok();
                        let cid = id.and_then(|d| d.get_str("id").ok()).unwrap_or("");
                        if cid.is_empty() {
                            continue;
                        }
                        agg.by_cashier.push((
                            cid.to_string(),
                            id.and_then(|d| d.get_str("name").ok())
                                .unwrap_or("Unknown")
                                .to_string(),
                            get_i64_flexible(row, "discount")?,
                            get_i64_flexible(row, "revenue")?,
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
                            get_i64_flexible(row, "discount")?,
                            get_i64_flexible(row, "units")?,
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
                "SELECT subtotal_cents, discount_cents, discount_type, total_cents, cashier_id, cashier_name_snapshot, items FROM invoices WHERE status != 'voided' AND created_at >= ? AND created_at < ?"
            )
            .bind(&start_iso)
            .bind(&end_iso)
            .fetch_all(pool)
            .await?;

            let mut agg = DiscountAgg::default();
            let mut by_type_map: std::collections::HashMap<String, (u64, i64)> =
                std::collections::HashMap::new();
            let mut by_cashier_map: std::collections::HashMap<String, (String, i64, i64)> =
                std::collections::HashMap::new();
            let mut top_products_map: std::collections::HashMap<
                (Option<String>, String),
                (i64, i64),
            > = std::collections::HashMap::new();

            for row in &rows {
                let doc = map_sqlite_row_to_document(row);
                let gross = get_i64_flexible(&doc, "subtotal_cents")?;
                let order_discount = get_i64_flexible(&doc, "discount_cents")?;
                let total = get_i64_flexible(&doc, "total_cents")?;
                let dtype = doc.get_str("discount_type").unwrap_or("none").to_string();
                let cid = doc.get_str("cashier_id").unwrap_or("").to_string();
                let cname = doc
                    .get_str("cashier_name_snapshot")
                    .unwrap_or("Unknown")
                    .to_string();

                agg.gross_before_discount_cents += gross;
                agg.order_level_discount_cents += order_discount;
                agg.invoice_count += 1;
                if order_discount > 0 {
                    agg.invoices_with_discount += 1;
                }

                let type_entry = by_type_map.entry(dtype).or_insert((0, 0));
                type_entry.0 += 1;
                type_entry.1 += order_discount;

                if !cid.is_empty() {
                    let c_entry = by_cashier_map.entry(cid.clone()).or_insert((cname, 0, 0));
                    c_entry.1 += order_discount;
                    c_entry.2 += total;
                }

                if let Ok(items) = doc.get_array("items") {
                    for item in items.iter().filter_map(|i| i.as_document()) {
                        let l_discount = if item.contains_key("discountCents") {
                            get_i64_flexible(item, "discountCents")?
                        } else {
                            get_i64_flexible(item, "discount_cents")?
                        };
                        agg.line_level_discount_cents += l_discount;

                        if l_discount > 0 {
                            let pk = item
                                .get_str("productKey")
                                .or_else(|_| item.get_str("product_key"))
                                .ok()
                                .map(String::from);
                            let name = item.get_str("name").unwrap_or("Unknown").to_string();
                            let qty = get_i64_flexible(item, "quantity")?;
                            let p_entry = top_products_map.entry((pk, name)).or_insert((0, 0));
                            p_entry.0 += l_discount;
                            p_entry.1 += qty;
                        }
                    }
                }
            }

            agg.by_type = by_type_map
                .into_iter()
                .map(|(t, (c, d))| (t, c, d))
                .collect();
            agg.by_cashier = by_cashier_map
                .into_iter()
                .map(|(cid, (name, d, rev))| (cid, name, d, rev))
                .collect();
            let mut top_prods: Vec<_> = top_products_map
                .into_iter()
                .map(|((pk, name), (d, q))| (pk, name, d, q))
                .collect();
            top_prods.sort_by_key(|b| std::cmp::Reverse(b.2));
            top_prods.truncate(10);
            agg.top_products = top_prods;

            Ok(agg)
        }
    }
}

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
                    agg.credit_note_count = get_i64_flexible(t, "count")? as u64;
                    agg.net_refund_cents = get_i64_flexible(t, "net")?;
                    agg.refund_cash_cents = get_i64_flexible(t, "cash")?;
                    agg.balance_reduction_cents = get_i64_flexible(t, "balance")?;
                    agg.no_receipt_count = get_i64_flexible(t, "noReceipt")? as u64;
                    agg.manager_override_count = get_i64_flexible(t, "override")? as u64;
                    agg.exchange_count = get_i64_flexible(t, "exchange")? as u64;
                }
                if let Ok(list) = result.get_array("byMethod") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_method.push((
                            required_text(row, "_id")?.to_string(),
                            get_i64_flexible(row, "amount")?,
                        ));
                    }
                }
                if let Ok(list) = result.get_array("byReason") {
                    for row in list.iter().filter_map(|r| r.as_document()) {
                        agg.by_reason.push((
                            row.get_str("_id").unwrap_or("other").to_string(),
                            get_i64_flexible(row, "count")? as u64,
                            get_i64_flexible(row, "amount")?,
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
                            get_i64_flexible(row, "qty")?,
                            get_i64_flexible(row, "amount")?,
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
                agg.net_refund_cents += get_i64_flexible(&doc, "net_refund_cents")?;
                agg.refund_cash_cents += get_i64_flexible(&doc, "refund_cash_cents")?;
                agg.balance_reduction_cents += get_i64_flexible(&doc, "balance_reduction_cents")?;
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
                        let m = required_text(item, "method")?.to_string();
                        let amt = get_i64_flexible(item, "amountCents")?;
                        *by_method_map.entry(m).or_insert(0) += amt;
                    }
                }

                if let Ok(ret_items) = doc.get_array("returned_items") {
                    for item in ret_items.iter().filter_map(|i| i.as_document()) {
                        let reason = item.get_str("reason").unwrap_or("other").to_string();
                        let amt = if item.contains_key("totalCents") {
                            get_i64_flexible(item, "totalCents")?
                        } else {
                            get_i64_flexible(item, "total_cents")?
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
                        let qty = get_i64_flexible(item, "quantity")?;
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

// ---------------------------------------------------------------------------
// discounts
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct DiscountAgg {
    pub gross_before_discount_cents: i64,
    pub order_level_discount_cents: i64,
    pub line_level_discount_cents: i64,
    pub invoice_count: u64,
    pub invoices_with_discount: u64,
    pub by_type: Vec<(String, u64, i64)>,
    pub by_cashier: Vec<(String, String, i64, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}

// ---------------------------------------------------------------------------
// refunds (detailed)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct RefundDetailAgg {
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<(String, u64, i64)>,
    pub by_method: Vec<(String, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}
