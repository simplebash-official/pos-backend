//! A print job's lines: checking what the print form sends, pricing stock parts on the server,
//! adding up the job, and taking the parts out of stock.

use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::applied_discount::AppliedDiscount;
use crate::domain::inventory::StockMovementType;
use crate::domain::print_jobs::{
    PrintItemSource, PrintItemSourceInput, PrintJob, PrintJobItem, PrintJobItemInput, PrintJobKind,
};
use crate::domain::warranties::ProductWarranty;
use crate::modules::inventory::service::job_parts::{self, JobPartPick, PlannedPart};
use pos_calc_core::item_discount::ItemDiscount;
use pos_calc_core::print_job::{
    MAX_LINE_QUANTITY, PrintItemMoney, line_discount_cents, print_job_totals,
};
use std::collections::HashSet;

const DESCRIPTION_MAX: usize = 200;
const PART_NAME_MAX: usize = 120;

/// A job's checked lines and what they add up to.
#[derive(Debug)]
pub(crate) struct PricedItems {
    pub items: Vec<PrintJobItem>,
    pub price_cents: i64,
    pub cost_cents: i64,
}

/// A line that passed every check that needs no stock.
#[derive(Debug)]
struct CheckedItem {
    key: String,
    description: String,
    quantity: i64,
    source: CheckedSource,
    discount: Option<ItemDiscount>,
    warranty: ProductWarranty,
}

/// A checked line source: priced by the form, or still to be found in stock.
#[derive(Debug)]
enum CheckedSource {
    Service {
        unit_price_cents: i64,
    },
    Outside {
        name: String,
        unit_price_cents: i64,
        unit_cost_cents: i64,
        /// What the whole line costs.
        cost_cents: i64,
    },
    Stock {
        product_key: String,
        serial_number: Option<String>,
        selling_price_cents: Option<i64>,
    },
}

fn trimmed(text: &str, what: &str, max: usize) -> AppResult<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::validation(format!("{what} is required.")));
    }
    if text.chars().count() > max {
        return Err(AppError::validation(format!(
            "{what} can be at most {max} characters."
        )));
    }
    Ok(text.to_string())
}

/// A typed unit amount: never negative, and 0 on a free rework.
fn unit_amount(cents: i64, what: &str, free: bool) -> AppResult<i64> {
    if cents < 0 {
        return Err(AppError::validation(format!("{what} cannot be negative.")));
    }
    if free && cents != 0 {
        return Err(AppError::validation(format!(
            "A warranty rework is free: {} must be 0.",
            what.to_lowercase()
        )));
    }
    Ok(cents)
}

/// Every check of the lines that needs no stock, so a bad list is refused before any lookup.
fn check_items(
    inputs: Vec<PrintJobItemInput>,
    existing: &[PrintJobItem],
    rework: bool,
) -> AppResult<Vec<CheckedItem>> {
    let mut keys: HashSet<String> = HashSet::new();
    let mut checked = Vec::with_capacity(inputs.len());
    for input in inputs {
        let key = match input.key {
            Some(key) if existing.iter().any(|item| item.key == key) => key,
            Some(key) => {
                return Err(AppError::validation(format!(
                    "Item {key} is not on this job."
                )));
            }
            None => generate_id(prefixes::PRINT_JOB_ITEM),
        };
        if !keys.insert(key.clone()) {
            return Err(AppError::validation(format!("Item {key} appears twice.")));
        }
        let description = trimmed(&input.description, "What is done", DESCRIPTION_MAX)?;
        let quantity = input.quantity;
        if !(1..=MAX_LINE_QUANTITY).contains(&quantity) {
            return Err(AppError::validation(format!(
                "A quantity must be between 1 and {MAX_LINE_QUANTITY}."
            )));
        }
        let warranty = input.warranty.validated()?;
        if rework && warranty.is_covered() {
            return Err(AppError::validation(
                "A warranty rework gives no new warranty; the original one still covers it.",
            ));
        }
        if rework && input.discount.is_some() {
            return Err(AppError::validation(
                "A warranty rework is free: it cannot have a discount.",
            ));
        }
        let source = match input.source {
            PrintItemSourceInput::Service { unit_price_cents } => CheckedSource::Service {
                unit_price_cents: unit_amount(unit_price_cents, "The fee", rework)?,
            },
            PrintItemSourceInput::Outside {
                name,
                unit_price_cents,
                unit_cost_cents,
            } => {
                let unit_price_cents = unit_amount(unit_price_cents, "The part price", rework)?;
                // What a bought-in part cost counts even on a free rework.
                let unit_cost_cents = unit_amount(unit_cost_cents, "The part cost", false)?;
                let cost_cents = unit_cost_cents.checked_mul(quantity).ok_or_else(|| {
                    AppError::validation("A print job amount cannot be negative or that large.")
                })?;
                CheckedSource::Outside {
                    name: trimmed(&name, "The part name", PART_NAME_MAX)?,
                    unit_price_cents,
                    unit_cost_cents,
                    cost_cents,
                }
            }
            PrintItemSourceInput::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            } => CheckedSource::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            },
        };
        checked.push(CheckedItem {
            key,
            description,
            quantity,
            source,
            discount: input.discount,
            warranty,
        });
    }
    Ok(checked)
}

/// How a part found in stock is kept on the line: a unit by its serial number, otherwise the
/// price tier it was taken from.
fn stock_source(found: PlannedPart) -> PrintItemSource {
    let selling_price_cents = found.serial.is_none().then_some(found.selling_price_cents);
    PrintItemSource::Stock {
        product_key: found.product_key,
        product_name: found.product_name,
        serial_number: found.serial.map(|serial| serial.serial_number),
        selling_price_cents,
    }
}

/// Checks the lines the print form sent and prices them. A line that names the key of one
/// already on the job keeps it; a new one gets a fresh key. A stock part is priced from the
/// unit or tier it names, and every unit asked for must be in stock now. A warranty rework
/// charges nothing and gives no new warranty, so its typed prices must be 0, its stock parts
/// are charged at 0 (their cost still counts) and it takes no discount.
pub(crate) async fn price_items(
    db: &Db,
    inputs: Vec<PrintJobItemInput>,
    existing: &[PrintJobItem],
    kind: &PrintJobKind,
) -> AppResult<PricedItems> {
    let rework = kind.is_rework();
    let checked = check_items(inputs, existing, rework)?;
    let picks: Vec<JobPartPick<'_>> = checked
        .iter()
        .filter_map(|item| match &item.source {
            CheckedSource::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            } => Some(JobPartPick {
                product_key,
                serial_number: serial_number.as_deref(),
                selling_price_cents: *selling_price_cents,
                quantity: item.quantity,
            }),
            CheckedSource::Service { .. } | CheckedSource::Outside { .. } => None,
        })
        .collect();
    let mut planned = job_parts::plan_job_parts(db, &picks, codes::PRINT_JOB_PART_INVALID)
        .await?
        .into_iter();
    let mut items = Vec::with_capacity(checked.len());
    for item in checked {
        let (source, unit_price_cents, cost_cents) = match item.source {
            CheckedSource::Service { unit_price_cents } => {
                (PrintItemSource::Service, unit_price_cents, 0)
            }
            CheckedSource::Outside {
                name,
                unit_price_cents,
                unit_cost_cents,
                cost_cents,
            } => (
                PrintItemSource::Outside {
                    name,
                    unit_cost_cents,
                },
                unit_price_cents,
                cost_cents,
            ),
            CheckedSource::Stock { .. } => {
                let found = planned.next().expect("every stock part was planned");
                let unit_price_cents = if rework { 0 } else { found.selling_price_cents };
                let cost_cents = found.cost_cents();
                (stock_source(found), unit_price_cents, cost_cents)
            }
        };
        let mut line = PrintJobItem {
            key: item.key,
            description: item.description,
            quantity: item.quantity,
            source,
            unit_price_cents,
            cost_cents,
            discount: None,
            warranty: item.warranty,
        };
        if let Some(typed) = item.discount {
            let amount_cents = line_discount_cents(&PrintItemMoney {
                discount: Some(typed),
                ..line.money()
            })?;
            line.discount = Some(AppliedDiscount {
                typed,
                amount_cents,
            });
        }
        items.push(line);
    }
    let money: Vec<PrintItemMoney> = items.iter().map(PrintJobItem::money).collect();
    let totals = print_job_totals(&money)?;
    Ok(PricedItems {
        items,
        price_cents: totals.price_cents,
        cost_cents: totals.cost_cents,
    })
}

/// The job's stock parts found in stock, in line order, without moving anything.
async fn plan_parts(db: &Db, job: &PrintJob) -> AppResult<Vec<PlannedPart>> {
    let mut picks = Vec::new();
    for item in &job.items {
        let PrintItemSource::Stock {
            product_key,
            serial_number,
            selling_price_cents,
            ..
        } = &item.source
        else {
            continue;
        };
        if serial_number.is_some() == selling_price_cents.is_some() {
            return Err(AppError::internal(format!(
                "print job {} item {} has a stock part without exactly one of a serial and a price",
                job.ticket_number, item.key
            )));
        }
        picks.push(JobPartPick {
            product_key,
            serial_number: serial_number.as_deref(),
            selling_price_cents: *selling_price_cents,
            quantity: item.quantity,
        });
    }
    job_parts::plan_job_parts(db, &picks, codes::PRINT_JOB_PART_INVALID).await
}

/// Checks every stock part of the job is still in stock, without moving anything. Billing calls
/// it before the invoice is written, so units sold in the meantime stop the sale.
pub(crate) async fn check_parts_in_stock(db: &Db, job: &PrintJob) -> AppResult<()> {
    plan_parts(db, job).await.map(|_| ())
}

/// Takes the job's stock parts out of stock: one movement per batch, tagged with
/// `movement_type` and `reference` (the invoice for a billed job, the job itself for a rework),
/// and a serialized part marked sold against that same reference. Everything is planned first,
/// so a part no longer in stock fails before any of them moves.
pub(crate) async fn use_parts(
    db: &Db,
    job: &PrintJob,
    movement_type: StockMovementType,
    reference: &str,
) -> AppResult<()> {
    let note = format!("Part used on print job {}", job.ticket_number);
    let parts = plan_parts(db, job).await?;
    job_parts::take_job_parts(db, parts, movement_type, reference, &note).await
}
