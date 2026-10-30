use super::compute::compute;
use crate::clients::db::Db;
use crate::core::calculations::share_basis_points;
use crate::core::error::AppResult;
use crate::domain::billing::{Invoice, InvoiceListQuery};
use crate::domain::employees::{Employee, EmployeeListQuery};
use crate::domain::inventory::{Product, ProductListQuery};
use crate::domain::print_jobs::{PrintJob, PrintJobListQuery};
use crate::domain::quotations::{Quotation, QuotationListQuery};
use crate::domain::repairs::{Repair, RepairListQuery};
use crate::domain::reports::DashboardPulse;
use crate::modules::{billing, employees, inventory, print_jobs, quotations, repairs};
use chrono::{DateTime, Utc};

pub(super) const ACTIVITY_PER_KIND: usize = 10;

pub(super) fn cost(estimate: Option<i64>, material: Option<i64>) -> i64 {
    estimate.unwrap_or(0) + material.unwrap_or(0)
}

const PAGE: u64 = 200;

/// Reads what the pulse needs and groups it. A part the caller may not see is left out (no
/// technicians without `employees:read`, no stock without `inventory:read`).
pub async fn get_dashboard_pulse(
    db: &Db,
    can_see_employees: bool,
    can_see_stock: bool,
    can_see_quotations: bool,
) -> AppResult<DashboardPulse> {
    let now = Utc::now();

    let mut repair_rows = Vec::new();
    for page in 1.. {
        let list = repairs::service::list_repairs(
            db,
            RepairListQuery {
                page: Some(page),
                limit: Some(PAGE),
                ..Default::default()
            },
        )
        .await?;
        let done = list.repairs.len() < PAGE as usize;
        repair_rows.extend(list.repairs);
        if done {
            break;
        }
    }
    let mut print_rows = Vec::new();
    for page in 1.. {
        let list = print_jobs::service::list_print_jobs(
            db,
            PrintJobListQuery {
                page: Some(page),
                limit: Some(PAGE),
                ..Default::default()
            },
        )
        .await?;
        let done = list.print_jobs.len() < PAGE as usize;
        print_rows.extend(list.print_jobs);
        if done {
            break;
        }
    }
    let mut product_rows = Vec::new();
    if can_see_stock {
        for page in 1.. {
            let list = inventory::service::product::list_products(
                db,
                ProductListQuery {
                    page: Some(page),
                    limit: Some(PAGE),
                    ..Default::default()
                },
            )
            .await?;
            let done = list.items.len() < PAGE as usize;
            product_rows.extend(list.items);
            if done {
                break;
            }
        }
    }
    let employee_rows = if can_see_employees {
        employees::service::list_employees(db, EmployeeListQuery::default())
            .await?
            .employees
    } else {
        Vec::new()
    };
    let mut today_rows = Vec::new();
    for page in 1.. {
        let list = billing::service::list_invoices(
            db,
            InvoiceListQuery {
                date_preset: Some("today".to_string()),
                page: Some(page),
                limit: Some(PAGE),
                ..Default::default()
            },
        )
        .await?;
        let done = list.invoices.len() < PAGE as usize;
        today_rows.extend(list.invoices);
        if done {
            break;
        }
    }
    let recent = billing::service::list_invoices(
        db,
        InvoiceListQuery {
            page: Some(1),
            limit: Some(ACTIVITY_PER_KIND as u64),
            ..Default::default()
        },
    )
    .await?;

    let mut quotation_rows = Vec::new();
    if can_see_quotations {
        for page in 1.. {
            let list = quotations::service::list_quotations(
                db,
                QuotationListQuery {
                    status: Some("active".to_string()),
                    page: Some(page),
                    limit: Some(PAGE),
                    ..Default::default()
                },
            )
            .await?;
            let done = list.quotations.len() < PAGE as usize;
            quotation_rows.extend(list.quotations);
            if done {
                break;
            }
        }
    }

    Ok(compute(&PulseInput {
        now,
        repairs: &repair_rows,
        print_jobs: &print_rows,
        products: &product_rows,
        employees: &employee_rows,
        today_invoices: &today_rows,
        recent_invoices: &recent.invoices,
        active_quotations: &quotation_rows,
    }))
}

pub(super) fn percent(count: i64, total: i64) -> i64 {
    (share_basis_points(count, total) + 50) / 100
}

#[derive(Debug)]
pub struct PulseInput<'a> {
    pub now: DateTime<Utc>,
    pub repairs: &'a [Repair],
    pub print_jobs: &'a [PrintJob],
    pub products: &'a [Product],
    pub employees: &'a [Employee],
    /// Invoices from today (voided ones are ignored here).
    pub today_invoices: &'a [Invoice],
    /// The latest invoices, for the activity stream.
    pub recent_invoices: &'a [Invoice],
    pub active_quotations: &'a [Quotation],
}
