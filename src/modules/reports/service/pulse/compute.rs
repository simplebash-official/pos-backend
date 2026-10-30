use super::service_pulse::ACTIVITY_PER_KIND;
use super::service_pulse::PulseInput;
use super::service_pulse::cost;
use super::service_pulse::percent;

use crate::core::utils::shop_day;
use crate::domain::billing::Invoice;
use crate::domain::employees::{EmployeeRole, EmployeeStatus};
use crate::domain::inventory::Product;
use crate::domain::print_jobs::PrintJob;

use crate::domain::repairs::Repair;
use crate::domain::reports::{
    DashboardPulse, PulseActivity, PulseFastMover, PulseLowStock, PulseQuotation,
    PulseQuotationItem, PulseReadyRepair, PulseStage, PulseTechnician,
};

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// A product with no low-stock line of its own counts as low at this many units.
const DEFAULT_LOW_STOCK_LINE: i64 = 5;

/// Active jobs that mean a technician is full.
const FULL_LOAD_JOBS: i64 = 5;

/// Days a finished repair can wait before it is flagged as critical.
const CRITICAL_WAIT_DAYS: i64 = 3;

const FAST_MOVERS_SHOWN: usize = 5;

const ACTIVITY_SHOWN: usize = 25;

fn stages(counts: &[(&str, i64)]) -> Vec<PulseStage> {
    let total: i64 = counts.iter().map(|(_, c)| c).sum();
    counts
        .iter()
        .map(|(stage, count)| PulseStage {
            stage: (*stage).to_string(),
            count: *count,
            percent: percent(*count, total),
        })
        .collect()
}

pub fn compute(input: &PulseInput) -> DashboardPulse {
    let today = |at: DateTime<Utc>| shop_day(at) == shop_day(input.now);
    let count = |status: &str, jobs: &mut dyn Iterator<Item = &str>| {
        jobs.filter(|s| *s == status).count() as i64
    };
    let repair_count =
        |status: &str| count(status, &mut input.repairs.iter().map(|r| r.status.as_str()));
    let print_count = |status: &str| {
        count(
            status,
            &mut input.print_jobs.iter().map(|p| p.status.as_str()),
        )
    };

    let ready: Vec<&Repair> = input
        .repairs
        .iter()
        .filter(|r| r.status == "ready")
        .collect();
    let is_active = |status: &str| matches!(status, "received" | "diagnosing" | "in_repair");

    let print_delivered_today = input
        .print_jobs
        .iter()
        .filter(|p| p.status == "delivered" && today(p.created_at))
        .count() as i64;

    let technicians = input
        .employees
        .iter()
        .filter(|e| {
            e.status == EmployeeStatus::Active
                && matches!(e.role, EmployeeRole::Technician | EmployeeRole::Printer)
        })
        .map(|tech| {
            let mine = |r: &&Repair| {
                r.assigned_employee_id.as_deref() == Some(tech.id.as_str())
                    || r.assigned_employee_name.as_deref() == Some(tech.name.as_str())
            };
            let active: Vec<&Repair> = input
                .repairs
                .iter()
                .filter(|r| is_active(&r.status))
                .filter(mine)
                .collect();
            let completed_today = input
                .repairs
                .iter()
                .filter(|r| {
                    matches!(r.status.as_str(), "ready" | "delivered") && today(r.created_at)
                })
                .filter(mine)
                .count() as i64;
            let active_jobs = active.len() as i64;
            PulseTechnician {
                employee_key: tech.key.clone(),
                name: tech.name.clone(),
                role: tech.role.as_str().to_string(),
                active_jobs_count: active_jobs,
                completed_today_count: completed_today,
                capacity_percent: (active_jobs * 100 / FULL_LOAD_JOBS).min(100),
                status: if active_jobs >= FULL_LOAD_JOBS {
                    "overloaded"
                } else if active_jobs >= 2 {
                    "busy"
                } else {
                    "available"
                }
                .to_string(),
                current_device_model: active.first().map(|r| r.device_model.clone()),
                current_ticket_number: active.first().map(|r| r.ticket_number.clone()),
            }
        })
        .collect();

    let products: HashMap<&str, &Product> =
        input.products.iter().map(|p| (p.key.as_str(), p)).collect();
    let mut sold: Vec<(String, PulseFastMover)> = Vec::new();
    for invoice in input
        .today_invoices
        .iter()
        .filter(|i| i.status.as_str() != "voided")
    {
        for item in invoice
            .items
            .iter()
            .filter(|i| i.source_type.is_empty() || i.source_type == "retail")
        {
            let key = item
                .product_key
                .clone()
                .unwrap_or_else(|| item.name.clone());
            if let Some((_, mover)) = sold.iter_mut().find(|(k, _)| *k == key) {
                mover.sold_today_units += item.quantity;
                continue;
            }
            let product = item.product_key.as_deref().and_then(|k| products.get(k));
            sold.push((
                key,
                PulseFastMover {
                    product_key: item.product_key.clone(),
                    name: product.map_or_else(|| item.name.clone(), |p| p.name.clone()),
                    sold_today_units: item.quantity,
                    selling_price_cents: item.unit_price_cents,
                    category: product.map(|p| p.category.clone()),
                    remaining_stock: product.map(|p| p.stock_quantity),
                    min_threshold: product.map(|p| p.min_stock_threshold),
                },
            ));
        }
    }
    let mut fast_movers: Vec<PulseFastMover> = sold.into_iter().map(|(_, m)| m).collect();
    fast_movers.sort_by(|a, b| {
        b.sold_today_units
            .cmp(&a.sold_today_units)
            .then(a.name.cmp(&b.name))
    });
    fast_movers.truncate(FAST_MOVERS_SHOWN);

    let low_stock = input
        .products
        .iter()
        .filter_map(|p| {
            let line = if p.min_stock_threshold > 0 {
                p.min_stock_threshold
            } else {
                DEFAULT_LOW_STOCK_LINE
            };
            (p.stock_quantity <= line).then(|| PulseLowStock {
                product_id: p.id.clone(),
                reference: if p.sku.is_empty() {
                    p.key.clone()
                } else {
                    p.sku.clone()
                },
                name: p.name.clone(),
                stock_quantity: p.stock_quantity,
                threshold: line,
                out_of_stock: p.stock_quantity <= 0,
                updated_at: p.updated_at,
                days_since_update: (input.now - p.updated_at).num_days().max(0),
            })
        })
        .collect();

    let ready_repairs = ready
        .iter()
        .map(|r| {
            let days = (input.now - r.created_at).num_days();
            PulseReadyRepair {
                id: r.id.clone(),
                ticket_number: r.ticket_number.clone(),
                device_model: r.device_model.clone(),
                customer_name: r.customer_name.clone(),
                customer_phone: r.customer_phone.clone(),
                created_at: r.created_at,
                days_waiting: days,
                severity: if days >= CRITICAL_WAIT_DAYS {
                    "critical"
                } else {
                    "warning"
                }
                .to_string(),
                amount_cents: cost(r.estimated_cost_cents, r.material_cost_cents),
            }
        })
        .collect();

    let mut sales: Vec<&Invoice> = input.recent_invoices.iter().collect();
    sales.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    let mut repairs_latest: Vec<&Repair> = input.repairs.iter().collect();
    repairs_latest.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    let mut prints_latest: Vec<&PrintJob> = input.print_jobs.iter().collect();
    prints_latest.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    let blank =
        |kind: &str, id: &str, reference: &str, amount_cents: i64, created_at| PulseActivity {
            kind: kind.to_string(),
            id: id.to_string(),
            reference: reference.to_string(),
            amount_cents,
            created_at,
            payment_method: None,
            cashier_name: None,
            status: None,
            device_model: None,
            customer_name: None,
            job_type: None,
            quantity: None,
        };
    let mut recent_activity: Vec<PulseActivity> = Vec::new();
    recent_activity.extend(sales.iter().take(ACTIVITY_PER_KIND).map(|i| PulseActivity {
        payment_method: Some(i.payment_method.clone()),
        cashier_name: Some(i.cashier_name_snapshot.clone()),
        ..blank(
            "sale",
            &i.id,
            &i.invoice_number,
            i.total_cents,
            i.created_at,
        )
    }));
    recent_activity.extend(
        repairs_latest
            .iter()
            .take(ACTIVITY_PER_KIND)
            .map(|r| PulseActivity {
                status: Some(r.status.clone()),
                device_model: Some(r.device_model.clone()),
                customer_name: Some(r.customer_name.clone()),
                ..blank(
                    "repair",
                    &r.id,
                    &r.ticket_number,
                    cost(r.estimated_cost_cents, r.material_cost_cents),
                    r.created_at,
                )
            }),
    );
    recent_activity.extend(
        prints_latest
            .iter()
            .take(ACTIVITY_PER_KIND)
            .map(|p| PulseActivity {
                job_type: Some(p.job_type.clone()),
                quantity: Some(i64::from(p.quantity)),
                customer_name: Some(p.customer_name.clone()),
                ..blank(
                    "print",
                    &p.id,
                    &p.ticket_number,
                    p.estimated_cost_cents,
                    p.created_at,
                )
            }),
    );
    recent_activity.sort_by_key(|a| std::cmp::Reverse(a.created_at));
    recent_activity.truncate(ACTIVITY_SHOWN);

    let pending_stock_quotations: Vec<PulseQuotation> = input
        .active_quotations
        .iter()
        .filter_map(|q| {
            let mut items = Vec::new();
            let mut has_out_of_stock = q.has_out_of_stock_items;
            for item in &q.items {
                let product = item.product_key.as_deref().and_then(|k| products.get(k));
                let stock_qty = product.map_or(0, |p| p.stock_quantity);
                let sufficient = stock_qty >= item.quantity;
                if !sufficient {
                    has_out_of_stock = true;
                }
                items.push(PulseQuotationItem {
                    product_key: item.product_key.clone(),
                    name: item.name.clone(),
                    sku: product.and_then(|p| {
                        if p.sku.is_empty() {
                            None
                        } else {
                            Some(p.sku.clone())
                        }
                    }),
                    quoted_quantity: item.quantity,
                    current_stock_quantity: stock_qty,
                    unit_price_cents: item.unit_price_cents,
                    has_sufficient_stock: sufficient,
                });
            }
            if !has_out_of_stock {
                return None;
            }
            let is_ready_to_fill = items.iter().all(|i| i.has_sufficient_stock);
            Some(PulseQuotation {
                key: q.key.clone(),
                quotation_number: q.quotation_number.clone(),
                customer_name: q.customer_name.clone(),
                customer_phone: q.customer_phone.clone(),
                total_cents: q.total_cents,
                created_at: q.created_at,
                valid_until: q.valid_until.clone(),
                items,
                is_ready_to_fill,
            })
        })
        .collect();

    DashboardPulse {
        active_repairs_count: input
            .repairs
            .iter()
            .filter(|r| is_active(&r.status))
            .count() as i64,
        ready_repairs_count: ready.len() as i64,
        uncollected_ready_value_cents: ready
            .iter()
            .map(|r| cost(r.estimated_cost_cents, r.material_cost_cents))
            .sum(),
        active_print_jobs_count: input
            .print_jobs
            .iter()
            .filter(|p| matches!(p.status.as_str(), "received" | "in_repair"))
            .count() as i64,
        repair_pipeline: stages(&[
            ("received", repair_count("received")),
            ("diagnosing", repair_count("diagnosing")),
            ("in_repair", repair_count("in_repair")),
            ("ready", ready.len() as i64),
        ]),
        print_pipeline: stages(&[
            ("received", print_count("received")),
            ("in_repair", print_count("in_repair")),
            ("ready", print_count("ready")),
            ("delivered", print_delivered_today),
        ]),
        technicians,
        fast_movers,
        low_stock,
        ready_repairs,
        recent_activity,
        pending_stock_quotations,
    }
}
