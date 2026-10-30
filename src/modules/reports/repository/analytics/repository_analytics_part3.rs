use super::super::super::service::dates::REPORT_TZ_MONGO;
use super::super::sales::get_i64_flexible;
use crate::clients::db::Db;
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::AppResult;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

// ---------------------------------------------------------------------------
// cashier performance
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct CashierAggRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub items_sold: i64,
    pub discount_given_cents: i64,
    pub credit_invoice_count: u64,
    pub gross_profit_cents: i64,
}

// ---------------------------------------------------------------------------
// sales by category
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct CategoryAggRow {
    pub category_key: Option<String>,
    pub category_name: String,
    pub subcategory_key: Option<String>,
    pub subcategory_name: Option<String>,
    pub units_sold: i64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
}

pub(super) fn date_match(start: DateTime<Utc>, end: DateTime<Utc>) -> Document {
    doc! {
        "status": { "$ne": "voided" },
        "created_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lt": BsonDateTime::from_chrono(end),
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

/// `customer.key -> outstanding_balance_cents` for the given keys.
pub(crate) async fn fetch_customer_balances(
    db: &Db,
    keys: &[String],
) -> AppResult<std::collections::HashMap<String, i64>> {
    match db {
        Db::Mongo(db) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            let mut cursor = db
                .collection::<Document>("customers")
                .find(doc! { "key": { "$in": keys } })
                .projection(doc! { "key": 1, "outstanding_balance_cents": 1 })
                .await?;
            while let Some(row) = cursor.try_next().await? {
                if let Ok(k) = row.get_str("key") {
                    out.insert(
                        k.to_string(),
                        get_i64_flexible(&row, "outstanding_balance_cents"),
                    );
                }
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut out = std::collections::HashMap::new();
            if keys.is_empty() {
                return Ok(out);
            }
            use sqlx::Row;
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT key, outstanding_balance_cents FROM customers WHERE key IN ({placeholders})"
            );
            let mut q = sqlx::query(&sql);
            for k in keys {
                q = q.bind(k);
            }
            let rows = q.fetch_all(pool).await?;
            for r in rows {
                let k: String = r.get("key");
                let bal: i64 = r.try_get("outstanding_balance_cents").unwrap_or(0);
                out.insert(k, bal);
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------------------
// KPI aggregate (one facet over `invoices`)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct InvoiceKpiAggregate {
    pub invoice_count: u64,
    pub gross_sales_cents: i64,
    pub discount_cents: i64,
    pub net_sales_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub retail_items_sold: i64,
    pub retail_cogs_cents: i64,
    /// Retail revenue on lines that carried a `unitCostCents` snapshot.
    pub retail_revenue_with_cost_cents: i64,
}

pub(super) fn invoices(db: &TenantDatabase) -> ScopedCollection<Document> {
    db.collection("invoices")
}

// ---------------------------------------------------------------------------
// Time series
// ---------------------------------------------------------------------------

/// One raw time bucket straight from Mongo, keyed by its start instant.
#[derive(Debug, Clone, Default)]
pub(crate) struct RawSeriesBucket {
    pub period_start: DateTime<Utc>,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub cogs_cents: i64,
    pub discount_cents: i64,
    pub invoice_count: u64,
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

// ---------------------------------------------------------------------------
// top customers
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct TopCustomerAggRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    /// retail line revenue − retail COGS for this customer (product margin only).
    pub gross_profit_cents: i64,
    pub last_purchase_at: DateTime<Utc>,
}

pub(super) fn trunc(unit: &str) -> Document {
    doc! {
        "$dateTrunc": {
            "date": "$created_at",
            "unit": unit,
            "binSize": 1,
            "timezone": REPORT_TZ_MONGO,
            "startOfWeek": "monday",
        }
    }
}
