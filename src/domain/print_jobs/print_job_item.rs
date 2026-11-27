//! The lines of a print job: a service fee, a part from stock or a part bought in, each with a
//! quantity, its own discount and its own warranty. The job's price and cost are added up from
//! the lines by `pos_calc_core::print_job`.
//!
//! These types are stored inside the print job document exactly as sent, so the camelCase field
//! names are also the stored names (the same rule as `RepairItem`).

use crate::domain::applied_discount::AppliedDiscount;
use crate::domain::warranties::ProductWarranty;
use pos_calc_core::item_discount::ItemDiscount;
use pos_calc_core::print_job::{PrintItemMoney, PrintLineKind};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a line sells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrintItemSource {
    /// Work the shop charges for, such as design or printing.
    #[serde(rename_all = "camelCase")]
    Service {
        /// What one unit costs the shop; absent when none is recorded.
        #[serde(skip_serializing_if = "Option::is_none")]
        unit_cost_cents: Option<i64>,
    },
    /// Units of a product the shop stocks. They leave stock when the job is billed.
    #[serde(rename_all = "camelCase")]
    Stock {
        product_key: String,
        /// The product's name when the part was chosen.
        product_name: String,
        /// The exact unit, for a product that tracks serial numbers.
        #[serde(skip_serializing_if = "Option::is_none")]
        serial_number: Option<String>,
        /// The price tier the units are taken from, for a product without serial numbers.
        #[serde(skip_serializing_if = "Option::is_none")]
        selling_price_cents: Option<i64>,
    },
    /// A part bought in for this job, never in stock.
    #[serde(rename_all = "camelCase")]
    Outside {
        /// What the part is.
        name: String,
        /// What one unit costs the shop.
        unit_cost_cents: i64,
    },
}

/// One line of a print job, as stored and returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobItem {
    /// Unique per job (`pit_…`); a warranty points at it.
    pub key: String,
    /// What the line is, e.g. "Mug printing".
    pub description: String,
    pub quantity: i64,
    pub source: PrintItemSource,
    /// What the customer pays for one unit; always 0 on a warranty rework.
    pub unit_price_cents: i64,
    /// What the whole line costs the shop; 0 for a service with no recorded cost. Worked out by
    /// the server.
    pub cost_cents: i64,
    /// The discount off this line; absent for none. Never on a warranty rework.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount: Option<AppliedDiscount>,
    /// The cover this line comes with once the job is handed back.
    pub warranty: ProductWarranty,
}

impl PrintJobItem {
    /// What the customer pays for this line: quantity times the price of one, less its discount.
    #[must_use]
    pub fn net_cents(&self) -> i64 {
        let discount_cents = match &self.discount {
            Some(discount) => discount.amount_cents,
            None => 0,
        };
        self.quantity * self.unit_price_cents - discount_cents
    }

    /// The line's money, as the totals are worked out from it.
    #[must_use]
    pub fn money(&self) -> PrintItemMoney {
        PrintItemMoney {
            kind: match self.source {
                PrintItemSource::Service { .. } => PrintLineKind::Service,
                PrintItemSource::Stock { .. } | PrintItemSource::Outside { .. } => {
                    PrintLineKind::Part
                }
            },
            quantity: self.quantity,
            unit_price_cents: self.unit_price_cents,
            cost_cents: self.cost_cents,
            discount: self.discount.as_ref().map(|discount| discount.typed),
        }
    }
}

/// What a line sells, as the print form sends it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PrintItemSourceInput {
    /// A service fee, charged per unit, with what one unit costs the shop when that is
    /// recorded.
    #[serde(rename_all = "camelCase")]
    Service {
        unit_price_cents: i64,
        unit_cost_cents: Option<i64>,
    },
    /// Units from stock: one `serialNumber` for a product that tracks serial numbers, otherwise
    /// the `sellingPriceCents` of one of its current price tiers. Price and cost come from stock,
    /// never from the client.
    #[serde(rename_all = "camelCase")]
    Stock {
        product_key: String,
        serial_number: Option<String>,
        selling_price_cents: Option<i64>,
    },
    /// A part bought in for this job, with what one unit is charged and what it cost.
    #[serde(rename_all = "camelCase")]
    Outside {
        name: String,
        unit_price_cents: i64,
        unit_cost_cents: i64,
    },
}

/// One line of a job, as the print form sends it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobItemInput {
    /// The key of a line already on the job, to keep its key when the job is edited.
    pub key: Option<String>,
    pub description: String,
    pub quantity: i64,
    pub source: PrintItemSourceInput,
    /// The discount typed on this line; absent for none. The server works out the amount.
    pub discount: Option<ItemDiscount>,
    pub warranty: ProductWarranty,
}

/// What a print job is: ordinary paid work, or free work redoing a job under its warranty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrintJobKind {
    Standard,
    /// Opened from a warranty claim on an earlier job. Charges nothing, gives no new warranty,
    /// and is handed back from the print job screen instead of being billed.
    #[serde(rename_all = "camelCase")]
    WarrantyRework {
        claim_key: String,
        claim_number: String,
        original_print_job_key: String,
        original_ticket_number: String,
    },
}

impl PrintJobKind {
    /// Whether this is free warranty work.
    #[must_use]
    pub fn is_rework(&self) -> bool {
        matches!(self, PrintJobKind::WarrantyRework { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_service_line_is_stored_with_its_cost_only_when_one_is_recorded() {
        for (unit_cost_cents, stored) in [
            (None, json!({ "type": "service" })),
            (Some(0), json!({ "type": "service", "unitCostCents": 0 })),
            (
                Some(400),
                json!({ "type": "service", "unitCostCents": 400 }),
            ),
        ] {
            let source = PrintItemSource::Service { unit_cost_cents };
            assert_eq!(
                serde_json::to_value(&source).expect("a source serializes"),
                stored
            );
            assert_eq!(
                serde_json::from_value::<PrintItemSource>(stored).expect("a source reads back"),
                source
            );
        }
    }
}
