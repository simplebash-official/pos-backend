//! The pure conversion from a print job's old two amounts to its lines, shared by the SQLite
//! startup migration, the MongoDB tool and the old-backup upgrader so all three produce the
//! same rows.

use crate::core::error::{AppError, AppResult};
use crate::domain::print_jobs::{PrintItemSource, PrintJobItem};
use crate::domain::warranties::ProductWarranty;
use pos_calc_core::print_job::{PrintItemMoney, print_job_totals};

/// What an old job becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedItems {
    pub items: Vec<PrintJobItem>,
    pub price_cents: i64,
    pub cost_cents: i64,
}

/// A job as it was stored before print jobs had lines.
#[derive(Debug, Clone, Copy)]
pub struct LegacyPrintJob<'a> {
    pub key: &'a str,
    pub job_type: &'a str,
    pub quantity: i64,
    /// What the customer was quoted.
    pub estimated_cost_cents: i64,
    /// What the blanks and ink cost the shop, when it was recorded.
    pub material_cost_cents: Option<i64>,
}

/// What the part line of a migrated job is called: the old material cost, kept as its cost.
pub const MIGRATED_MATERIALS_NAME: &str = "Materials";

/// The key of a migrated job's service line, the same on every device and in the cloud.
#[must_use]
pub fn migrated_service_key(job_key: &str) -> String {
    format!("pit_mig_{job_key}")
}

/// The key of a migrated job's materials line.
#[must_use]
pub fn migrated_materials_key(job_key: &str) -> String {
    format!("pit_mig_mat_{job_key}")
}

/// The job type as a line description says it; a stored type outside the shop's set is kept as
/// it is stored.
fn type_title(job_type: &str) -> &str {
    match job_type {
        "mug" => "Mug",
        "t-shirt" => "T-shirt",
        "handbill" => "Handbill",
        "banner" => "Banner",
        "custom" => "Custom",
        other => other,
    }
}

/// The old `estimated_cost_cents` was what the customer was quoted (the print form, the till and
/// every report read it that way) and `material_cost_cents` what the materials cost the shop. A
/// quote becomes one service line of that price, and a material cost one bought-in "Materials"
/// line charged at nothing, so the job's price and cost stay exactly what they were. A job with
/// neither amount above zero (not quoted yet) gets no lines. No warranty: old jobs never had one.
///
/// # Errors
///
/// An amount that is negative or too large.
pub fn legacy_items(job: LegacyPrintJob<'_>) -> AppResult<MigratedItems> {
    if job.estimated_cost_cents < 0 || job.material_cost_cents.is_some_and(|cents| cents < 0) {
        return Err(AppError::internal(format!(
            "print job {} holds a negative amount",
            job.key
        )));
    }
    let mut items = Vec::new();
    if job.estimated_cost_cents > 0 {
        items.push(PrintJobItem {
            key: migrated_service_key(job.key),
            description: format!(
                "{} printing ({} units)",
                type_title(job.job_type),
                job.quantity
            ),
            quantity: 1,
            source: PrintItemSource::Service {
                unit_cost_cents: None,
            },
            unit_price_cents: job.estimated_cost_cents,
            cost_cents: 0,
            discount: None,
            warranty: ProductWarranty::none(),
        });
    }
    if let Some(material_cost_cents) = job.material_cost_cents.filter(|cents| *cents > 0) {
        items.push(PrintJobItem {
            key: migrated_materials_key(job.key),
            description: MIGRATED_MATERIALS_NAME.to_string(),
            quantity: 1,
            source: PrintItemSource::Outside {
                name: MIGRATED_MATERIALS_NAME.to_string(),
                unit_cost_cents: material_cost_cents,
            },
            unit_price_cents: 0,
            cost_cents: material_cost_cents,
            discount: None,
            warranty: ProductWarranty::none(),
        });
    }
    let money: Vec<PrintItemMoney> = items.iter().map(PrintJobItem::money).collect();
    let totals = print_job_totals(&money)
        .map_err(|err| AppError::internal(format!("print job {}: {err}", job.key)))?;
    Ok(MigratedItems {
        items,
        price_cents: totals.price_cents,
        cost_cents: totals.cost_cents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(estimated_cost_cents: i64, material_cost_cents: Option<i64>) -> MigratedItems {
        legacy_items(LegacyPrintJob {
            key: "prn_1",
            job_type: "mug",
            quantity: 12,
            estimated_cost_cents,
            material_cost_cents,
        })
        .expect("in range")
    }

    #[test]
    fn a_quote_and_materials_keep_the_same_price_and_cost() {
        let migrated = job(2_500_000, Some(1_300_000));
        assert_eq!(migrated.price_cents, 2_500_000);
        assert_eq!(migrated.cost_cents, 1_300_000);
        assert_eq!(migrated.items.len(), 2);
        let service = &migrated.items[0];
        assert_eq!(service.key, "pit_mig_prn_1");
        assert_eq!(service.description, "Mug printing (12 units)");
        assert_eq!(service.quantity, 1);
        assert_eq!(
            service.source,
            PrintItemSource::Service {
                unit_cost_cents: None
            }
        );
        assert!(!service.warranty.is_covered());
        let materials = &migrated.items[1];
        assert_eq!(materials.key, "pit_mig_mat_prn_1");
        assert_eq!(materials.unit_price_cents, 0);
        assert_eq!(materials.cost_cents, 1_300_000);
    }

    #[test]
    fn a_quote_without_materials_is_one_service_line() {
        for material in [None, Some(0)] {
            let migrated = job(50_000, material);
            assert_eq!(migrated.items.len(), 1);
            assert_eq!(migrated.price_cents, 50_000);
            assert_eq!(migrated.cost_cents, 0);
        }
    }

    #[test]
    fn materials_without_a_quote_are_one_line_charged_at_nothing() {
        let migrated = job(0, Some(900));
        assert_eq!(migrated.items.len(), 1);
        assert_eq!(migrated.price_cents, 0);
        assert_eq!(migrated.cost_cents, 900);
    }

    #[test]
    fn a_job_never_quoted_gets_no_lines() {
        let migrated = job(0, None);
        assert!(migrated.items.is_empty());
        assert_eq!(migrated.price_cents, 0);
    }

    #[test]
    fn a_negative_old_amount_is_refused() {
        for (estimated, material) in [(-1, None), (100, Some(-1))] {
            assert!(
                legacy_items(LegacyPrintJob {
                    key: "prn_2",
                    job_type: "banner",
                    quantity: 1,
                    estimated_cost_cents: estimated,
                    material_cost_cents: material,
                })
                .is_err()
            );
        }
    }
}
