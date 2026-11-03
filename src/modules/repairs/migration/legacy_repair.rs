//! The pure conversion from a repair job's old two amounts to its items, shared by the SQLite
//! startup migration, the MongoDB tool and the old-backup upgrader so all three produce the
//! same rows.

use crate::core::error::{AppError, AppResult};
use crate::domain::repairs::{RepairItem, RepairPart};
use crate::domain::warranties::ProductWarranty;
use pos_calc_core::repair::{RepairItemMoney, repair_totals};

/// What an old job becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedItems {
    pub items: Vec<RepairItem>,
    pub price_cents: i64,
    pub cost_cents: i64,
}

/// The item key a migrated job's one item gets, the same on every device and in the cloud.
#[must_use]
pub fn migrated_item_key(repair_key: &str) -> String {
    format!("rit_mig_{repair_key}")
}

/// What the part line of a migrated job is called: the old material cost, kept as its cost.
pub const MIGRATED_MATERIALS_NAME: &str = "Materials";

/// The old `estimated_cost_cents` was what the customer was quoted (the repair form, the till
/// and every report read it that way) and `material_cost_cents` what the parts cost the shop.
/// A job with either becomes one item: the quote as labour and the materials as an outside part
/// charged at nothing, so the job's price and cost stay exactly what they were. A job with
/// neither (not quoted yet) gets no items. No warranty: old jobs never had one.
///
/// # Errors
///
/// An amount that is negative or too large.
pub fn legacy_items(
    repair_key: &str,
    issue_description: &str,
    estimated_cost_cents: Option<i64>,
    material_cost_cents: Option<i64>,
) -> AppResult<MigratedItems> {
    if estimated_cost_cents.is_none() && material_cost_cents.is_none() {
        return Ok(MigratedItems {
            items: Vec::new(),
            price_cents: 0,
            cost_cents: 0,
        });
    }
    let labor_cents = estimated_cost_cents.unwrap_or(0);
    let part_cost_cents = material_cost_cents.unwrap_or(0);
    let totals = repair_totals(&[RepairItemMoney {
        labor_cents,
        part_cents: 0,
        part_cost_cents,
    }])
    .map_err(|err| AppError::internal(format!("repair {repair_key}: {err}")))?;
    let description = match issue_description.trim() {
        "" => "Repair".to_string(),
        text => text.chars().take(200).collect(),
    };
    Ok(MigratedItems {
        items: vec![RepairItem {
            key: migrated_item_key(repair_key),
            description,
            labor_cents,
            part: (part_cost_cents > 0).then(|| RepairPart::Outside {
                name: MIGRATED_MATERIALS_NAME.to_string(),
            }),
            part_cents: 0,
            part_cost_cents,
            warranty: ProductWarranty::none(),
        }],
        price_cents: totals.price_cents,
        cost_cents: totals.cost_cents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_and_materials_keep_the_same_price_and_cost() {
        let migrated =
            legacy_items("rep_1", "Cracked screen", Some(1_500_000), Some(900_000)).expect("ok");
        assert_eq!(migrated.price_cents, 1_500_000);
        assert_eq!(migrated.cost_cents, 900_000);
        assert_eq!(migrated.items.len(), 1);
        let item = &migrated.items[0];
        assert_eq!(item.key, "rit_mig_rep_1");
        assert_eq!(item.description, "Cracked screen");
        assert_eq!(item.part_cents, 0);
        assert!(matches!(item.part, Some(RepairPart::Outside { .. })));
        assert!(!item.warranty.is_covered());
    }

    #[test]
    fn a_quote_without_materials_has_no_part() {
        let migrated = legacy_items("rep_2", "  ", Some(50_000), None).expect("ok");
        assert_eq!(migrated.items[0].description, "Repair");
        assert_eq!(migrated.items[0].part, None);
        assert_eq!(migrated.cost_cents, 0);
    }

    #[test]
    fn a_job_never_quoted_gets_no_items() {
        let migrated = legacy_items("rep_3", "No power", None, None).expect("ok");
        assert!(migrated.items.is_empty());
        assert_eq!(migrated.price_cents, 0);
    }

    #[test]
    fn a_negative_old_amount_is_refused() {
        assert!(legacy_items("rep_4", "x", Some(-1), None).is_err());
    }
}
