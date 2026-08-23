// MongoDB aggregation and calculation of technician & staff commissions across repairs and print jobs.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
};

use crate::{
    core::error::AppResult,
    domain::reports::{EmployeeCommissionsReportResponse, EmployeePerformanceEntry},
};

fn repairs(db: &Database) -> Collection<Document> {
    db.collection("repairs")
}

fn print_jobs(db: &Database) -> Collection<Document> {
    db.collection("print_jobs")
}

fn get_i64_flexible(doc: &Document, key: &str) -> i64 {
    doc.get_i64(key)
        .ok()
        .or_else(|| doc.get_i32(key).ok().map(i64::from))
        .or_else(|| doc.get_f64(key).ok().map(|v| v as i64))
        .unwrap_or(0)
}

pub(crate) async fn aggregate_employee_commissions(
    db: &Database,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    filter_employee_name: Option<&str>,
) -> AppResult<EmployeeCommissionsReportResponse> {
    let mut perf_map: std::collections::BTreeMap<String, EmployeePerformanceEntry> =
        std::collections::BTreeMap::new();

    let mut repair_match = doc! {
        "deleted_at": null,
        "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
        "created_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lt": BsonDateTime::from_chrono(end),
        }
    };
    if let Some(emp) = filter_employee_name {
        repair_match.insert("assigned_employee_name", emp);
    }

    let mut cursor = repairs(db).find(repair_match).await?;
    while let Some(doc) = cursor.try_next().await? {
        let emp_name = doc
            .get_str("assigned_employee_name")
            .unwrap_or("Unassigned Technician")
            .to_string();
        let revenue = get_i64_flexible(&doc, "estimated_cost_cents");
        let material_cost = get_i64_flexible(&doc, "material_cost_cents");
        let profit = (revenue - material_cost).max(0);

        let split_type = doc.get_str("split_type").ok();
        let split_val = doc
            .get_f64("split_value")
            .ok()
            .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
            .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
            .unwrap_or(0.0);

        let earned = match split_type {
            Some("percentage") => ((profit as f64 * split_val) / 100.0).round() as i64,
            Some("fixed") => split_val.round() as i64,
            _ => 0,
        };
        let net_contribution = (profit - earned).max(0);

        let entry = perf_map
            .entry(emp_name.clone())
            .or_insert_with(|| EmployeePerformanceEntry {
                employee_name: emp_name,
                role: "technician".to_string(),
                assigned_jobs_count: 0,
                revenue_generated_cents: 0,
                estimated_cost_cents: 0,
                profit_cents: 0,
                earned_commission_cents: 0,
                net_shop_contribution_cents: 0,
            });

        entry.assigned_jobs_count += 1;
        entry.revenue_generated_cents += revenue;
        entry.estimated_cost_cents += material_cost;
        entry.profit_cents += profit;
        entry.earned_commission_cents += earned;
        entry.net_shop_contribution_cents += net_contribution;
    }

    let mut print_match = doc! {
        "deleted_at": null,
        "status": { "$in": ["completed", "delivered", "ready_for_pickup"] },
        "created_at": {
            "$gte": BsonDateTime::from_chrono(start),
            "$lt": BsonDateTime::from_chrono(end),
        }
    };
    if let Some(emp) = filter_employee_name {
        print_match.insert("assigned_employee_name", emp);
    }

    let mut cursor = print_jobs(db).find(print_match).await?;
    while let Some(doc) = cursor.try_next().await? {
        let emp_name = doc
            .get_str("assigned_employee_name")
            .unwrap_or("Unassigned Printer")
            .to_string();
        let revenue = get_i64_flexible(&doc, "estimated_cost_cents");
        let material_cost = get_i64_flexible(&doc, "material_cost_cents");
        let profit = (revenue - material_cost).max(0);

        let split_type = doc.get_str("split_type").ok();
        let split_val = doc
            .get_f64("split_value")
            .ok()
            .or_else(|| doc.get_i64("split_value").ok().map(|v| v as f64))
            .or_else(|| doc.get_i32("split_value").ok().map(|v| v as f64))
            .unwrap_or(0.0);

        let earned = match split_type {
            Some("percentage") => ((profit as f64 * split_val) / 100.0).round() as i64,
            Some("fixed") => split_val.round() as i64,
            _ => 0,
        };
        let net_contribution = (profit - earned).max(0);

        let entry = perf_map
            .entry(emp_name.clone())
            .or_insert_with(|| EmployeePerformanceEntry {
                employee_name: emp_name,
                role: "printer".to_string(),
                assigned_jobs_count: 0,
                revenue_generated_cents: 0,
                estimated_cost_cents: 0,
                profit_cents: 0,
                earned_commission_cents: 0,
                net_shop_contribution_cents: 0,
            });

        entry.assigned_jobs_count += 1;
        entry.revenue_generated_cents += revenue;
        entry.estimated_cost_cents += material_cost;
        entry.profit_cents += profit;
        entry.earned_commission_cents += earned;
        entry.net_shop_contribution_cents += net_contribution;
    }

    let mut total_commissions_cents = 0;
    let mut total_revenue_cents = 0;
    let mut total_jobs_count = 0;

    let employees: Vec<EmployeePerformanceEntry> = perf_map.into_values().collect();
    for emp in &employees {
        total_commissions_cents += emp.earned_commission_cents;
        total_revenue_cents += emp.revenue_generated_cents;
        total_jobs_count += emp.assigned_jobs_count;
    }

    Ok(EmployeeCommissionsReportResponse {
        employees,
        total_commissions_cents,
        total_revenue_cents,
        total_jobs_count,
        period_start: start,
        period_end: end,
    })
}
