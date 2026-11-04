//! The pieces of work on a repair job. Each item is labour plus at most one part, with its own
//! warranty, so a screen replacement and a cleaning on one job can carry different cover. The
//! job's price and cost are added up from the items by `pos_calc_core::repair`.
//!
//! These types are stored inside the repair document exactly as sent, so the camelCase field
//! names are also the stored names (the same rule as `ProductWarranty` and `InvoiceItem`).

use crate::domain::warranties::ProductWarranty;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where an item's part came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum RepairPart {
    /// A unit of a product the shop stocks. It leaves stock when the job is handed back.
    #[serde(rename_all = "camelCase")]
    Stock {
        product_key: String,
        /// The product's name when the part was chosen.
        product_name: String,
        /// The exact unit, for a product that tracks serial numbers.
        #[serde(skip_serializing_if = "Option::is_none")]
        serial_number: Option<String>,
        /// The price tier the unit is taken from, for a product without serial numbers.
        #[serde(skip_serializing_if = "Option::is_none")]
        selling_price_cents: Option<i64>,
    },
    /// A part bought in for this job, never in stock.
    #[serde(rename_all = "camelCase")]
    Outside {
        /// What the part is.
        name: String,
    },
}

/// One piece of work on a job, as stored and returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairItem {
    /// Unique per job (`rit_…`); a warranty points at it.
    pub key: String,
    /// What is being done, e.g. "Replace screen".
    pub description: String,
    /// What the work is charged.
    pub labor_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<RepairPart>,
    /// What the customer pays for the part; 0 without one, and always 0 on a warranty rework.
    pub part_cents: i64,
    /// What the part costs the shop; 0 without one. Worked out by the server for a stock part.
    pub part_cost_cents: i64,
    /// The cover this work comes with once the job is handed back.
    pub warranty: ProductWarranty,
}

/// Which part an item uses, as the repair form sends it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum RepairPartInput {
    /// A unit from stock: its `serialNumber` for a product that tracks serial numbers,
    /// otherwise the `sellingPriceCents` of one of its current price tiers. Price and cost come
    /// from that unit or tier, never from the client.
    #[serde(rename_all = "camelCase")]
    Stock {
        product_key: String,
        serial_number: Option<String>,
        selling_price_cents: Option<i64>,
    },
    /// A part bought in for this job, with what it is charged and what it cost.
    #[serde(rename_all = "camelCase")]
    Outside {
        name: String,
        price_cents: i64,
        cost_cents: i64,
    },
}

/// One item of a job, as the repair form sends it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairItemInput {
    /// The key of an item already on the job, to keep its key when the job is edited.
    pub key: Option<String>,
    pub description: String,
    pub labor_cents: i64,
    pub part: Option<RepairPartInput>,
    pub warranty: ProductWarranty,
}

/// What a job is: ordinary paid work, or free work redoing a job under its warranty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairKind {
    Standard,
    /// Opened from a warranty claim on an earlier job. Charges nothing, gives no new warranty,
    /// and is handed back from the repair screen instead of being billed.
    #[serde(rename_all = "camelCase")]
    WarrantyRework {
        claim_key: String,
        claim_number: String,
        original_repair_key: String,
        original_ticket_number: String,
    },
}

impl RepairKind {
    /// Whether this is free warranty work.
    #[must_use]
    pub fn is_rework(&self) -> bool {
        matches!(self, RepairKind::WarrantyRework { .. })
    }
}
