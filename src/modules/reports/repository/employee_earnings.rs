// MongoDB-only itemized commission listing for one employee — the
// server-computed replacement for the frontend's former mock earnings
// ledger. Reuses `commissions.rs`'s `compute_earned_cents` formula so the
// aggregated and itemized views can never silently disagree. Lives here
// (not in `modules::employees`) because it reads raw `repairs`/`print_jobs`
// documents directly, the same cross-collection financial-aggregation
// responsibility `reports` already owns for `commissions.rs`.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    domain::employees::EmployeeEarningRecord,
    modules::reports::repository::commissions::{compute_earned_cents, get_i64_flexible},
};

fn repairs(db: &Database) -> Collection<Document> {
    db.collection("repairs")
}

fn print_jobs(db: &Database) -> Collection<Document> {
    db.collection("print_jobs")
}

fn to_record(
    doc: &Document,
    work_type: &'static str,
    description: String,
) -> EmployeeEarningRecord {
    let revenue = get_i64_flexible(doc, "estimated_cost_cents");
    let material_cost = get_i64_flexible(doc, "material_cost_cents");
    let profit = (revenue - material_cost).max(0);

    let split_type = doc.get_str("split_type").ok();
    let split_val = doc
        .get_f64("split_value")
        .ok()
        .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
        .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
        .unwrap_or(0.0);
    let earned = compute_earned_cents(profit, split_type, split_val);

    let status = if doc.get_str("status").ok() == Some("delivered") {
        "completed"
    } else {
        "pending"
    };

    EmployeeEarningRecord {
        work_id: doc.get_str("key").unwrap_or_default().to_string(),
        ticket_or_invoice_number: doc.get_str("ticket_number").unwrap_or_default().to_string(),
        work_type: work_type.to_string(),
        description,
        customer_name: doc.get_str("customer_name").unwrap_or_default().to_string(),
        total_amount_cents: revenue,
        cost_cents: material_cost,
        profit_cents: profit,
        split_type: split_type.unwrap_or_default().to_string(),
        split_value: split_val,
        earned_amount_cents: earned,
        status: status.to_string(),
        created_at: doc
            .get_datetime("created_at")
            .map(|d| d.to_chrono())
            .unwrap_or_else(|_| Utc::now()),
    }
}

/// Every completed/delivered repair and print job assigned to `employee_key`
/// within `[start, end)`, newest first.
pub(crate) async fn list_employee_earnings(
    db: &Db,
    employee_key: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<Vec<EmployeeEarningRecord>> {
    let mut records = Vec::new();

    match db {
        Db::Mongo(db) => {
            let repair_match = doc! {
                "deleted_at": null,
                "assigned_employee_id": employee_key,
                "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                "created_at": {
                    "$gte": BsonDateTime::from_chrono(start),
                    "$lt": BsonDateTime::from_chrono(end),
                }
            };
            let mut cursor = repairs(db)
                .find(repair_match)
                .sort(doc! { "created_at": -1 })
                .await?;
            while let Some(doc) = cursor.try_next().await? {
                let description = doc
                    .get_str("issue_description")
                    .unwrap_or_default()
                    .to_string();
                records.push(to_record(&doc, "repair", description));
            }

            let print_match = doc! {
                "deleted_at": null,
                "assigned_employee_id": employee_key,
                "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
                "created_at": {
                    "$gte": BsonDateTime::from_chrono(start),
                    "$lt": BsonDateTime::from_chrono(end),
                }
            };
            let mut cursor = print_jobs(db)
                .find(print_match)
                .sort(doc! { "created_at": -1 })
                .await?;
            while let Some(doc) = cursor.try_next().await? {
                let description = doc.get_str("job_type").unwrap_or_default().to_string();
                records.push(to_record(&doc, "print", description));
            }
        }
        Db::Sqlite(pool) => {
            let start_iso = start.to_rfc3339();
            let end_iso = end.to_rfc3339();

            let repair_query = "SELECT * FROM repairs WHERE deleted_at IS NULL AND assigned_employee_id = ? AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ? ORDER BY created_at DESC";
            let repair_rows = sqlx::query(repair_query)
                .bind(employee_key)
                .bind(&start_iso)
                .bind(&end_iso)
                .fetch_all(pool)
                .await?;
            for row in repair_rows {
                let doc = map_sqlite_row_to_document(&row);
                let description = doc
                    .get_str("issue_description")
                    .unwrap_or_default()
                    .to_string();
                records.push(to_record(&doc, "repair", description));
            }

            let print_query = "SELECT * FROM print_jobs WHERE deleted_at IS NULL AND assigned_employee_id = ? AND status IN ('completed', 'delivered', 'ready_for_pickup') AND created_at >= ? AND created_at < ? ORDER BY created_at DESC";
            let print_rows = sqlx::query(print_query)
                .bind(employee_key)
                .bind(&start_iso)
                .bind(&end_iso)
                .fetch_all(pool)
                .await?;
            for row in print_rows {
                let doc = map_sqlite_row_to_document(&row);
                let description = doc.get_str("job_type").unwrap_or_default().to_string();
                records.push(to_record(&doc, "print", description));
            }
        }
    }

    records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(records)
}
