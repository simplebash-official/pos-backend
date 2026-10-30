use super::activity_per_kind::ACTIVITY_PER_KIND;
use super::compute::compute;
use super::pulse_input::PulseInput;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::billing::InvoiceListQuery;
use crate::domain::employees::EmployeeListQuery;
use crate::domain::inventory::ProductListQuery;
use crate::domain::print_jobs::PrintJobListQuery;
use crate::domain::quotations::QuotationListQuery;
use crate::domain::repairs::RepairListQuery;
use crate::domain::reports::DashboardPulse;
use crate::modules::{billing, employees, inventory, print_jobs, quotations, repairs};
use chrono::Utc;

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
