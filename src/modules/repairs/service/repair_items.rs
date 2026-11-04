//! A job's items: checking what the repair form sends, pricing stock parts on the server, adding
//! up the job, and taking the parts out of stock when the job is handed back.

use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::{BatchAllocation, StockMovementType};
use crate::domain::repairs::{
    Repair, RepairItem, RepairItemInput, RepairKind, RepairPart, RepairPartInput,
};
use crate::modules::inventory;
use crate::modules::inventory::model::ProductSerialDocument;
use pos_calc_core::repair::{RepairItemMoney, repair_totals};
use std::collections::HashSet;

const DESCRIPTION_MAX: usize = 200;
const PART_NAME_MAX: usize = 120;

/// A job's checked items and what they add up to.
#[derive(Debug)]
pub(crate) struct PricedItems {
    pub items: Vec<RepairItem>,
    pub price_cents: i64,
    pub cost_cents: i64,
}

fn part_invalid(message: impl Into<String>) -> AppError {
    AppError::validation_with_code(message.into(), codes::REPAIR_PART_INVALID)
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

fn not_negative(cents: i64, what: &str) -> AppResult<()> {
    if cents < 0 {
        return Err(AppError::validation(format!("{what} cannot be negative.")));
    }
    Ok(())
}

/// A stock part's price and cost, from the unit or tier it names.
async fn price_stock_part(
    db: &Db,
    product_key: &str,
    serial_number: Option<&str>,
    selling_price_cents: Option<i64>,
) -> AppResult<(RepairPart, i64, i64)> {
    let product = inventory::service::product::get_product_by_key(db, product_key).await?;
    match (product.is_serialized, serial_number, selling_price_cents) {
        (true, Some(serial_number), None) => {
            let serial = inventory::service::product_serial::resolve_in_stock_serial(
                db,
                &product.key,
                serial_number,
            )
            .await?;
            let (price, allocations) = inventory::service::stock_batch::allocations_for_serials(
                db,
                &product.key,
                &product.name,
                std::slice::from_ref(&serial.batch_key),
            )
            .await?;
            let cost = inventory::service::stock_batch::weighted_unit_cost(&allocations);
            Ok((
                RepairPart::Stock {
                    product_key: product.key,
                    product_name: product.name,
                    serial_number: Some(serial.serial_number),
                    selling_price_cents: None,
                },
                price,
                cost,
            ))
        }
        (true, _, _) => Err(part_invalid(format!(
            "Scan the serial number of the {} used.",
            product.name
        ))),
        (false, None, Some(price)) => {
            let allocations = inventory::service::stock_batch::plan_tier_allocation(
                db,
                &product.key,
                &product.name,
                price,
                1,
            )
            .await?;
            let cost = inventory::service::stock_batch::weighted_unit_cost(&allocations);
            Ok((
                RepairPart::Stock {
                    product_key: product.key,
                    product_name: product.name,
                    serial_number: None,
                    selling_price_cents: Some(price),
                },
                price,
                cost,
            ))
        }
        (false, _, _) => Err(part_invalid(format!(
            "Choose which price of {} the part is taken from.",
            product.name
        ))),
    }
}

/// Checks the items the repair form sent and prices them. An item that names the key of one
/// already on the job keeps it; a new one gets a fresh key. A stock part is priced from the
/// unit or tier it names, which must be in stock now. A warranty rework charges nothing and
/// gives no new warranty, so its labour and outside-part prices must be 0 and its stock parts
/// are charged at 0 (their cost still counts).
pub(crate) async fn price_items(
    db: &Db,
    inputs: Vec<RepairItemInput>,
    existing: &[RepairItem],
    kind: &RepairKind,
) -> AppResult<PricedItems> {
    let rework = kind.is_rework();
    let mut keys: HashSet<String> = HashSet::new();
    let mut serials: HashSet<String> = HashSet::new();
    let mut items = Vec::with_capacity(inputs.len());
    for input in inputs {
        let key = match input.key {
            Some(key) if existing.iter().any(|item| item.key == key) => key,
            Some(key) => {
                return Err(AppError::validation(format!(
                    "Item {key} is not on this job."
                )));
            }
            None => generate_id(prefixes::REPAIR_ITEM),
        };
        if !keys.insert(key.clone()) {
            return Err(AppError::validation(format!("Item {key} appears twice.")));
        }
        let description = trimmed(&input.description, "What is done", DESCRIPTION_MAX)?;
        not_negative(input.labor_cents, "Labour")?;
        if rework && input.labor_cents != 0 {
            return Err(AppError::validation(
                "A warranty rework is free: its labour must be 0.",
            ));
        }
        let warranty = input.warranty.validated()?;
        if rework && warranty.is_covered() {
            return Err(AppError::validation(
                "A warranty rework gives no new warranty; the original one still covers it.",
            ));
        }
        let (part, part_cents, part_cost_cents) = match input.part {
            None => (None, 0, 0),
            Some(RepairPartInput::Outside {
                name,
                price_cents,
                cost_cents,
            }) => {
                not_negative(price_cents, "The part price")?;
                not_negative(cost_cents, "The part cost")?;
                if rework && price_cents != 0 {
                    return Err(AppError::validation(
                        "A warranty rework is free: its part price must be 0.",
                    ));
                }
                let name = trimmed(&name, "The part name", PART_NAME_MAX)?;
                (Some(RepairPart::Outside { name }), price_cents, cost_cents)
            }
            Some(RepairPartInput::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            }) => {
                if let Some(serial) = &serial_number
                    && !serials.insert(serial.clone())
                {
                    return Err(part_invalid(format!(
                        "Serial number {serial} is used on two items."
                    )));
                }
                let (part, price, cost) = price_stock_part(
                    db,
                    &product_key,
                    serial_number.as_deref(),
                    selling_price_cents,
                )
                .await?;
                (Some(part), if rework { 0 } else { price }, cost)
            }
        };
        items.push(RepairItem {
            key,
            description,
            labor_cents: input.labor_cents,
            part,
            part_cents,
            part_cost_cents,
            warranty,
        });
    }
    let money: Vec<RepairItemMoney> = items
        .iter()
        .map(|item| RepairItemMoney {
            labor_cents: item.labor_cents,
            part_cents: item.part_cents,
            part_cost_cents: item.part_cost_cents,
        })
        .collect();
    let totals = repair_totals(&money)?;
    Ok(PricedItems {
        items,
        price_cents: totals.price_cents,
        cost_cents: totals.cost_cents,
    })
}

/// One stock part, found in stock and planned, before anything moves.
#[derive(Debug)]
struct PlannedPart {
    product_key: String,
    product_name: String,
    serial: Option<ProductSerialDocument>,
    allocations: Vec<BatchAllocation>,
}

async fn plan_parts(db: &Db, repair: &Repair) -> AppResult<Vec<PlannedPart>> {
    let mut planned = Vec::new();
    for item in &repair.items {
        let Some(RepairPart::Stock {
            product_key,
            product_name,
            serial_number,
            selling_price_cents,
        }) = &item.part
        else {
            continue;
        };
        match (serial_number, selling_price_cents) {
            (Some(serial_number), None) => {
                let serial = inventory::service::product_serial::resolve_in_stock_serial(
                    db,
                    product_key,
                    serial_number,
                )
                .await?;
                let (_, allocations) = inventory::service::stock_batch::allocations_for_serials(
                    db,
                    product_key,
                    product_name,
                    std::slice::from_ref(&serial.batch_key),
                )
                .await?;
                planned.push(PlannedPart {
                    product_key: product_key.clone(),
                    product_name: product_name.clone(),
                    serial: Some(serial),
                    allocations,
                });
            }
            (None, Some(price)) => {
                let allocations = inventory::service::stock_batch::plan_tier_allocation(
                    db,
                    product_key,
                    product_name,
                    *price,
                    1,
                )
                .await?;
                planned.push(PlannedPart {
                    product_key: product_key.clone(),
                    product_name: product_name.clone(),
                    serial: None,
                    allocations,
                });
            }
            _ => {
                return Err(AppError::internal(format!(
                    "repair {} item {} has a stock part with neither a serial nor a price",
                    repair.ticket_number, item.key
                )));
            }
        }
    }
    Ok(planned)
}

/// Checks every stock part of the job is still in stock, without moving anything. Billing calls
/// it before the invoice is written, so a part sold in the meantime stops the sale.
pub(crate) async fn check_parts_in_stock(db: &Db, repair: &Repair) -> AppResult<()> {
    plan_parts(db, repair).await.map(|_| ())
}

/// Takes the job's stock parts out of stock: one movement per batch, tagged with
/// `movement_type` and `reference` (the invoice for a billed job, the job itself for a rework),
/// and a serialized part marked sold against that same reference. Everything is planned first,
/// so a part no longer in stock fails before any of them moves.
pub(crate) async fn use_parts(
    db: &Db,
    repair: &Repair,
    movement_type: StockMovementType,
    reference: &str,
) -> AppResult<()> {
    let note = format!("Part used on repair {}", repair.ticket_number);
    for part in plan_parts(db, repair).await? {
        inventory::service::stock_batch::move_allocations(
            db,
            &part.product_key,
            &part.allocations,
            -1,
            movement_type,
            reference,
            &note,
        )
        .await?;
        if let Some(serial) = part.serial {
            let serial_id = serial.id.ok_or_else(|| {
                AppError::internal(format!(
                    "serial {} of {} has no stored id",
                    serial.serial_number, part.product_name
                ))
            })?;
            inventory::service::product_serial::mark_serial_sold(db, serial_id, reference).await?;
        }
    }
    Ok(())
}
