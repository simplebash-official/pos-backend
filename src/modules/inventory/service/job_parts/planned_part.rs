//! One part found in stock for a job, and the planning of a job's parts together.

use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::inventory::{BatchAllocation, StockMovementType};
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::service::{product, product_serial, stock_batch};
use std::collections::{BTreeMap, HashSet};

/// One part as a job names it: an exact unit of a product that tracks serial numbers, or a
/// quantity from one price tier of a product that does not.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JobPartPick<'a> {
    pub product_key: &'a str,
    pub serial_number: Option<&'a str>,
    pub selling_price_cents: Option<i64>,
    pub quantity: i64,
}

/// A pick found in stock: what it sells at and which batches it draws on.
#[derive(Debug)]
pub(crate) struct PlannedPart {
    pub product_key: String,
    pub product_name: String,
    /// The exact unit, for a product that tracks serial numbers.
    pub serial: Option<ProductSerialDocument>,
    /// What one unit sells at: the unit's batch price, or the tier's.
    pub selling_price_cents: i64,
    pub allocations: Vec<BatchAllocation>,
}

impl PlannedPart {
    /// What all the units of this part cost the shop.
    #[must_use]
    pub fn cost_cents(&self) -> i64 {
        self.allocations
            .iter()
            .map(|allocation| allocation.quantity * allocation.unit_cost_cents)
            .sum()
    }
}

/// The picks drawing on one price tier of one product.
#[derive(Debug)]
struct Tier {
    product_name: String,
    /// Positions in the request, in order.
    picks: Vec<usize>,
}

/// Shares a tier's planned batches out among the lines that asked for them, in order, oldest
/// batch first. The allocations must hold exactly the units the lines ask for.
fn split_allocations(
    allocations: &[BatchAllocation],
    quantities: &[i64],
) -> Vec<Vec<BatchAllocation>> {
    let mut remaining = allocations.iter();
    let mut current: Option<BatchAllocation> = None;
    quantities
        .iter()
        .map(|&quantity| {
            let mut left = quantity;
            let mut share = Vec::new();
            while left > 0 {
                let mut batch = match current.take() {
                    Some(batch) => batch,
                    None => remaining
                        .next()
                        .expect("the plan covers every unit the lines ask for")
                        .clone(),
                };
                let take = left.min(batch.quantity);
                share.push(BatchAllocation {
                    quantity: take,
                    ..batch.clone()
                });
                left -= take;
                batch.quantity -= take;
                if batch.quantity > 0 {
                    current = Some(batch);
                }
            }
            share
        })
        .collect()
}

/// Finds every pick in stock, without moving anything, and returns them in the order given.
/// Picks of the same product and price are planned together, so lines that each fit what is
/// left but do not fit it together are refused. A pick that does not suit its product (a missing
/// serial number or price, a serial used twice) is a validation error with `invalid_code`.
pub(crate) async fn plan_job_parts(
    db: &Db,
    picks: &[JobPartPick<'_>],
    invalid_code: &str,
) -> AppResult<Vec<PlannedPart>> {
    let invalid =
        |message: String| AppError::validation_with_code(message, invalid_code.to_string());
    let mut serials: HashSet<&str> = HashSet::new();
    let mut planned: Vec<Option<PlannedPart>> = Vec::with_capacity(picks.len());
    let mut tiers: BTreeMap<(String, i64), Tier> = BTreeMap::new();
    for (position, pick) in picks.iter().enumerate() {
        if let Some(serial_number) = pick.serial_number
            && !serials.insert(serial_number)
        {
            return Err(invalid(format!(
                "Serial number {serial_number} is used on two items."
            )));
        }
        let found = product::get_product_by_key(db, pick.product_key).await?;
        match (
            found.is_serialized,
            pick.serial_number,
            pick.selling_price_cents,
        ) {
            (true, Some(serial_number), None) => {
                if pick.quantity != 1 {
                    return Err(invalid(format!(
                        "Each {} has its own serial number: put one on each item.",
                        found.name
                    )));
                }
                let serial =
                    product_serial::resolve_in_stock_serial(db, &found.key, serial_number).await?;
                let (price, allocations) = stock_batch::allocations_for_serials(
                    db,
                    &found.key,
                    &found.name,
                    std::slice::from_ref(&serial.batch_key),
                )
                .await?;
                planned.push(Some(PlannedPart {
                    product_key: found.key,
                    product_name: found.name,
                    serial: Some(serial),
                    selling_price_cents: price,
                    allocations,
                }));
            }
            (true, _, _) => {
                return Err(invalid(format!(
                    "Scan the serial number of the {} used.",
                    found.name
                )));
            }
            (false, None, Some(price)) => {
                tiers
                    .entry((found.key, price))
                    .or_insert_with(|| Tier {
                        product_name: found.name,
                        picks: Vec::new(),
                    })
                    .picks
                    .push(position);
                planned.push(None);
            }
            (false, _, _) => {
                return Err(invalid(format!(
                    "Choose which price of {} the part is taken from.",
                    found.name
                )));
            }
        }
    }
    for ((product_key, price), tier) in tiers {
        let quantities: Vec<i64> = tier.picks.iter().map(|&i| picks[i].quantity).collect();
        let total = quantities
            .iter()
            .try_fold(0_i64, |sum, &quantity| sum.checked_add(quantity))
            .ok_or_else(|| invalid("That many units cannot be taken from stock.".to_string()))?;
        let allocations =
            stock_batch::plan_tier_allocation(db, &product_key, &tier.product_name, price, total)
                .await?;
        let shares = split_allocations(&allocations, &quantities);
        for (position, allocations) in tier.picks.into_iter().zip(shares) {
            planned[position] = Some(PlannedPart {
                product_key: product_key.clone(),
                product_name: tier.product_name.clone(),
                serial: None,
                selling_price_cents: price,
                allocations,
            });
        }
    }
    Ok(planned
        .into_iter()
        .map(|part| part.expect("every pick was planned by itself or with its tier"))
        .collect())
}

/// Takes planned parts out of stock: one movement per batch, tagged with `movement_type` and
/// `reference`, and a serialized unit marked sold against that same reference.
pub(crate) async fn take_job_parts(
    db: &Db,
    parts: Vec<PlannedPart>,
    movement_type: StockMovementType,
    reference: &str,
    note: &str,
) -> AppResult<()> {
    for part in parts {
        stock_batch::move_allocations(
            db,
            &part.product_key,
            &part.allocations,
            -1,
            movement_type,
            reference,
            note,
        )
        .await?;
        if let Some(serial) = part.serial {
            let serial_id = serial.id.ok_or_else(|| {
                AppError::internal(format!(
                    "serial {} of {} has no stored id",
                    serial.serial_number, part.product_name
                ))
            })?;
            product_serial::mark_serial_sold(db, serial_id, reference).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(batch_key: &str, quantity: i64, unit_cost_cents: i64) -> BatchAllocation {
        BatchAllocation {
            batch_key: batch_key.to_string(),
            quantity,
            unit_cost_cents,
        }
    }

    fn units(share: &[BatchAllocation]) -> Vec<(&str, i64)> {
        share
            .iter()
            .map(|a| (a.batch_key.as_str(), a.quantity))
            .collect()
    }

    #[test]
    fn lines_share_a_tiers_batches_in_order() {
        let plan = [batch("old", 5, 100), batch("new", 10, 130)];
        let shares = split_allocations(&plan, &[3, 4, 8]);
        assert_eq!(units(&shares[0]), vec![("old", 3)]);
        assert_eq!(units(&shares[1]), vec![("old", 2), ("new", 2)]);
        assert_eq!(units(&shares[2]), vec![("new", 8)]);
        assert_eq!(shares[1][0].unit_cost_cents, 100);
        assert_eq!(shares[1][1].unit_cost_cents, 130);
    }

    #[test]
    fn a_part_costs_every_unit_at_its_own_batch_cost() {
        let part = PlannedPart {
            product_key: "prod_1".to_string(),
            product_name: "Mug".to_string(),
            serial: None,
            selling_price_cents: 500,
            allocations: vec![batch("old", 2, 100), batch("new", 1, 101)],
        };
        assert_eq!(part.cost_cents(), 301);
    }
}
