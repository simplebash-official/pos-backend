// The dashboard's groupings and totals: the repair and print pipelines, technician load, what is
// selling today, what is low or out of stock, repairs waiting for pickup and the activity stream.
// `compute` is pure; `get_dashboard_pulse` reads the records it needs. Wording, colours and
// "time ago" stay on the screen: this sends figures, codes and dates.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::{
    clients::db::Db,
    core::{calculations::share_basis_points, error::AppResult, utils::shop_day},
    domain::{
        billing::{Invoice, InvoiceListQuery},
        employees::{Employee, EmployeeListQuery, EmployeeRole, EmployeeStatus},
        inventory::{Product, ProductListQuery},
        print_jobs::{PrintJob, PrintJobListQuery},
        quotations::{Quotation, QuotationListQuery},
        repairs::{Repair, RepairListQuery},
        reports::{
            DashboardPulse, PulseActivity, PulseFastMover, PulseLowStock, PulseQuotation,
            PulseQuotationItem, PulseReadyRepair, PulseStage, PulseTechnician,
        },
    },
    modules::{billing, employees, inventory, print_jobs, quotations, repairs},
};

/// A product with no low-stock line of its own counts as low at this many units.
const DEFAULT_LOW_STOCK_LINE: i64 = 5;
/// Active jobs that mean a technician is full.
const FULL_LOAD_JOBS: i64 = 5;
/// Days a finished repair can wait before it is flagged as critical.
const CRITICAL_WAIT_DAYS: i64 = 3;
const FAST_MOVERS_SHOWN: usize = 5;
const ACTIVITY_PER_KIND: usize = 10;
const ACTIVITY_SHOWN: usize = 25;
const PAGE: u64 = 200;

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

fn percent(count: i64, total: i64) -> i64 {
    (share_basis_points(count, total) + 50) / 100
}

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

fn cost(estimate: Option<i64>, material: Option<i64>) -> i64 {
    estimate.unwrap_or(0) + material.unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const NOW: &str = "2026-10-26T12:00:00Z";

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("{e}"))
    }

    fn repair(id: &str, status: &str, tech: Option<&str>, created: &str, cost: i64) -> Repair {
        from(json!({
            "id": id, "key": id, "ticketNumber": format!("REP-{id}"), "customerName": "Asha",
            "customerPhone": "0771234567", "deviceModel": "Phone", "issueDescription": "x",
            "isOverdue": false, "status": status, "estimatedCostCents": cost,
            "materialCostCents": 500, "assignedEmployeeId": tech, "createdAt": created,
            "updatedAt": created
        }))
    }

    fn employee(id: &str, role: &str) -> Employee {
        from(json!({
            "id": id, "key": format!("emp_{id}"), "name": format!("Name {id}"), "phone": "0770000000", "role": role,
            "defaultSplitType": "percentage", "defaultSplitValue": 30.0, "status": "active",
            "createdAt": NOW, "updatedAt": NOW, "version": 1
        }))
    }

    fn input<'a>(repairs: &'a [Repair], employees: &'a [Employee]) -> PulseInput<'a> {
        PulseInput {
            now: NOW.parse().unwrap(),
            repairs,
            print_jobs: &[],
            products: &[],
            employees,
            today_invoices: &[],
            recent_invoices: &[],
            active_quotations: &[],
        }
    }

    #[test]
    fn repairs_are_grouped_into_the_pipeline_with_rounded_shares() {
        let repairs = [
            repair("a", "received", None, NOW, 1_000),
            repair("b", "received", None, NOW, 1_000),
            repair("c", "in_repair", None, NOW, 1_000),
        ];
        let pulse = compute(&input(&repairs, &[]));
        let counts: Vec<(i64, i64)> = pulse
            .repair_pipeline
            .iter()
            .map(|s| (s.count, s.percent))
            .collect();
        assert_eq!(counts, [(2, 67), (0, 0), (1, 33), (0, 0)]);
        assert_eq!(pulse.active_repairs_count, 3);
    }

    #[test]
    fn a_ready_repair_waiting_three_days_is_critical_and_counts_towards_the_waiting_value() {
        let repairs = [
            repair("a", "ready", None, "2026-10-23T11:00:00Z", 10_000),
            repair("b", "ready", None, "2026-10-26T08:00:00Z", 2_000),
        ];
        let pulse = compute(&input(&repairs, &[]));
        assert_eq!(pulse.uncollected_ready_value_cents, 10_500 + 2_500);
        assert_eq!(pulse.ready_repairs[0].severity, "critical");
        assert_eq!(pulse.ready_repairs[0].days_waiting, 3);
        assert_eq!(pulse.ready_repairs[1].severity, "warning");
    }

    #[test]
    fn a_technician_is_busy_from_two_jobs_and_overloaded_at_five() {
        let employees = [employee("t1", "technician"), employee("s1", "sales")];
        let mut repairs: Vec<Repair> = (0..5)
            .map(|i| repair(&format!("j{i}"), "in_repair", Some("t1"), NOW, 100))
            .collect();
        let busy = compute(&input(&repairs[..2], &employees));
        assert_eq!(busy.technicians.len(), 1);
        assert_eq!(busy.technicians[0].status, "busy");
        assert_eq!(busy.technicians[0].capacity_percent, 40);
        repairs.push(repair("done", "ready", Some("t1"), NOW, 100));
        let full = compute(&input(&repairs, &employees));
        assert_eq!(full.technicians[0].status, "overloaded");
        assert_eq!(full.technicians[0].capacity_percent, 100);
        assert_eq!(full.technicians[0].completed_today_count, 1);
    }

    #[test]
    fn pending_stock_quotations_are_tracked_and_flagged_ready_when_replenished() {
        let quote: Quotation = from(json!({
            "id": "q1",
            "key": "quo_1",
            "quotationNumber": "QUO-0001",
            "customerName": "Kamal",
            "items": [{
                "productKey": "prod_1",
                "name": "Screen Protector",
                "quantity": 3,
                "unitPriceCents": 1500,
                "discountCents": 0,
                "totalCents": 4500
            }],
            "subtotalCents": 4500,
            "discountType": "fixed",
            "discountValue": 0.0,
            "discountCents": 0,
            "totalCents": 4500,
            "validUntil": "2026-11-26",
            "status": "active",
            "isExpired": false,
            "hasOutOfStockItems": true,
            "createdAt": NOW,
            "updatedAt": NOW
        }));

        let empty_stock_product: Product = from(json!({
            "id": "p1", "key": "prod_1", "sku": "SP-01", "name": "Screen Protector",
            "categoryKey": "cat_1", "category": "Accessories",
            "subcategoryKey": "sub_1", "subcategory": "Protectors",
            "stockQuantity": 0, "minStockThreshold": 5,
            "priceTiers": [],
            "createdAt": NOW, "updatedAt": NOW, "version": 1
        }));

        let mut inp = input(&[], &[]);
        inp.active_quotations = std::slice::from_ref(&quote);
        inp.products = std::slice::from_ref(&empty_stock_product);

        let pulse_waiting = compute(&inp);
        assert_eq!(pulse_waiting.pending_stock_quotations.len(), 1);
        assert!(!pulse_waiting.pending_stock_quotations[0].is_ready_to_fill);
        assert!(!pulse_waiting.pending_stock_quotations[0].items[0].has_sufficient_stock);

        // When stock is replenished to 5:
        let replenished_product: Product = from(json!({
            "id": "p1", "key": "prod_1", "sku": "SP-01", "name": "Screen Protector",
            "categoryKey": "cat_1", "category": "Accessories",
            "subcategoryKey": "sub_1", "subcategory": "Protectors",
            "stockQuantity": 5, "minStockThreshold": 5,
            "priceTiers": [],
            "createdAt": NOW, "updatedAt": NOW, "version": 1
        }));
        inp.products = std::slice::from_ref(&replenished_product);

        let pulse_ready = compute(&inp);
        assert_eq!(pulse_ready.pending_stock_quotations.len(), 1);
        assert!(pulse_ready.pending_stock_quotations[0].is_ready_to_fill);
        assert!(pulse_ready.pending_stock_quotations[0].items[0].has_sufficient_stock);
    }
}
