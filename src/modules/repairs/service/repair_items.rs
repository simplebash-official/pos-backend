//! A job's items: checking what the repair form sends, pricing stock parts on the server, adding
//! up the job, and taking the parts out of stock when the job is handed back.

use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::applied_discount::AppliedDiscount;
use crate::domain::inventory::StockMovementType;
use crate::domain::repairs::{
    Repair, RepairItem, RepairItemInput, RepairKind, RepairPart, RepairPartInput,
};
use crate::domain::warranties::ProductWarranty;
use crate::modules::inventory::service::job_parts::{self, JobPartPick, PlannedPart};
use pos_calc_core::item_discount::ItemDiscount;
use pos_calc_core::repair::{RepairItemMoney, item_discount_cents, repair_totals};
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

/// An item that passed every check that needs no stock.
#[derive(Debug)]
struct CheckedItem {
    key: String,
    description: String,
    labor_cents: i64,
    labor_cost_cents: Option<i64>,
    part: Option<CheckedPart>,
    discount: Option<ItemDiscount>,
    warranty: ProductWarranty,
}

/// A checked part: bought in, or still to be found in stock.
#[derive(Debug)]
enum CheckedPart {
    Outside {
        name: String,
        price_cents: i64,
        cost_cents: i64,
    },
    Stock {
        product_key: String,
        serial_number: Option<String>,
        selling_price_cents: Option<i64>,
    },
}

/// Every check of the items that needs no stock, so a bad list is refused before any lookup.
fn check_items(
    inputs: Vec<RepairItemInput>,
    existing: &[RepairItem],
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
            None => generate_id(prefixes::REPAIR_ITEM),
        };
        if !keys.insert(key.clone()) {
            return Err(AppError::validation(format!("Item {key} appears twice.")));
        }
        let description = trimmed(&input.description, "What is done", DESCRIPTION_MAX)?;
        not_negative(input.labor_cents, "Fee")?;
        // What the work cost counts even on a free rework.
        if let Some(cost_cents) = input.labor_cost_cents {
            not_negative(cost_cents, "The cost")?;
        }
        if rework && input.labor_cents != 0 {
            return Err(AppError::validation(
                "A warranty rework is free: its fees must be 0.",
            ));
        }
        let warranty = input.warranty.validated()?;
        if rework && warranty.is_covered() {
            return Err(AppError::validation(
                "A warranty rework gives no new warranty; the original one still covers it.",
            ));
        }
        let part = match input.part {
            None => None,
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
                Some(CheckedPart::Outside {
                    name,
                    price_cents,
                    cost_cents,
                })
            }
            Some(RepairPartInput::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            }) => Some(CheckedPart::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            }),
        };
        if rework && input.discount.is_some() {
            return Err(AppError::validation(
                "A warranty rework is free: it cannot have a discount.",
            ));
        }
        checked.push(CheckedItem {
            key,
            description,
            labor_cents: input.labor_cents,
            labor_cost_cents: input.labor_cost_cents,
            part,
            discount: input.discount,
            warranty,
        });
    }
    Ok(checked)
}

/// How a part found in stock is kept on the item: a unit by its serial number, otherwise the
/// price tier it was taken from.
fn stock_part(found: PlannedPart) -> RepairPart {
    let selling_price_cents = found.serial.is_none().then_some(found.selling_price_cents);
    RepairPart::Stock {
        product_key: found.product_key,
        product_name: found.product_name,
        serial_number: found.serial.map(|serial| serial.serial_number),
        selling_price_cents,
    }
}

/// Checks the items the repair form sent and prices them. An item that names the key of one
/// already on the job keeps it; a new one gets a fresh key. A stock part is priced from the
/// unit or tier it names, which must be in stock now. A warranty rework charges nothing and
/// gives no new warranty, so its labour and outside-part prices must be 0 and its stock parts
/// are charged at 0 (labour and part costs still count) and it takes no discount.
pub(crate) async fn price_items(
    db: &Db,
    inputs: Vec<RepairItemInput>,
    existing: &[RepairItem],
    kind: &RepairKind,
) -> AppResult<PricedItems> {
    let rework = kind.is_rework();
    let checked = check_items(inputs, existing, rework)?;
    let picks: Vec<JobPartPick<'_>> = checked
        .iter()
        .filter_map(|item| match &item.part {
            Some(CheckedPart::Stock {
                product_key,
                serial_number,
                selling_price_cents,
            }) => Some(JobPartPick {
                product_key,
                serial_number: serial_number.as_deref(),
                selling_price_cents: *selling_price_cents,
                quantity: 1,
            }),
            Some(CheckedPart::Outside { .. }) | None => None,
        })
        .collect();
    let mut planned = job_parts::plan_job_parts(db, &picks, codes::REPAIR_PART_INVALID)
        .await?
        .into_iter();
    let mut items = Vec::with_capacity(checked.len());
    for item in checked {
        let (part, part_cents, part_cost_cents) = match item.part {
            None => (None, 0, 0),
            Some(CheckedPart::Outside {
                name,
                price_cents,
                cost_cents,
            }) => (Some(RepairPart::Outside { name }), price_cents, cost_cents),
            Some(CheckedPart::Stock { .. }) => {
                let found = planned.next().expect("every stock part was planned");
                let price_cents = if rework { 0 } else { found.selling_price_cents };
                let cost_cents = found.cost_cents();
                (Some(stock_part(found)), price_cents, cost_cents)
            }
        };
        let discount = match item.discount {
            Some(typed) => {
                let amount_cents = item_discount_cents(&RepairItemMoney {
                    labor_cents: item.labor_cents,
                    labor_cost_cents: item.labor_cost_cents,
                    part_cents,
                    part_cost_cents,
                    discount: Some(typed),
                })?;
                Some(AppliedDiscount {
                    typed,
                    amount_cents,
                })
            }
            None => None,
        };
        items.push(RepairItem {
            key: item.key,
            description: item.description,
            labor_cents: item.labor_cents,
            labor_cost_cents: item.labor_cost_cents,
            part,
            part_cents,
            part_cost_cents,
            discount,
            warranty: item.warranty,
        });
    }
    let money: Vec<RepairItemMoney> = items
        .iter()
        .map(|item| RepairItemMoney {
            labor_cents: item.labor_cents,
            labor_cost_cents: item.labor_cost_cents,
            part_cents: item.part_cents,
            part_cost_cents: item.part_cost_cents,
            discount: item.discount.as_ref().map(|discount| discount.typed),
        })
        .collect();
    let totals = repair_totals(&money)?;
    Ok(PricedItems {
        items,
        price_cents: totals.price_cents,
        cost_cents: totals.cost_cents,
    })
}

/// The job's stock parts found in stock, in item order, without moving anything.
async fn plan_parts(db: &Db, repair: &Repair) -> AppResult<Vec<PlannedPart>> {
    let mut picks = Vec::new();
    for item in &repair.items {
        let Some(RepairPart::Stock {
            product_key,
            serial_number,
            selling_price_cents,
            ..
        }) = &item.part
        else {
            continue;
        };
        if serial_number.is_some() == selling_price_cents.is_some() {
            return Err(AppError::internal(format!(
                "repair {} item {} has a stock part without exactly one of a serial and a price",
                repair.ticket_number, item.key
            )));
        }
        picks.push(JobPartPick {
            product_key,
            serial_number: serial_number.as_deref(),
            selling_price_cents: *selling_price_cents,
            quantity: 1,
        });
    }
    job_parts::plan_job_parts(db, &picks, codes::REPAIR_PART_INVALID).await
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
    let parts = plan_parts(db, repair).await?;
    job_parts::take_job_parts(db, parts, movement_type, reference, &note).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fee(labor_cents: i64, labor_cost_cents: Option<i64>) -> RepairItemInput {
        RepairItemInput {
            key: None,
            description: "Board repair".to_string(),
            labor_cents,
            labor_cost_cents,
            part: None,
            discount: None,
            warranty: ProductWarranty::none(),
        }
    }

    #[test]
    fn a_free_rework_still_records_what_the_work_cost() {
        let checked = check_items(vec![fee(0, Some(700))], &[], true).expect("a rework may cost");
        assert_eq!(checked[0].labor_cost_cents, Some(700));
    }

    #[test]
    fn no_recorded_cost_is_not_a_cost_of_zero() {
        let checked = check_items(vec![fee(100, None), fee(100, Some(0))], &[], false)
            .expect("both are valid");
        assert_eq!(checked[0].labor_cost_cents, None);
        assert_eq!(checked[1].labor_cost_cents, Some(0));
    }

    #[test]
    fn refuses_a_negative_cost() {
        assert!(check_items(vec![fee(100, Some(-1))], &[], false).is_err());
    }
}
