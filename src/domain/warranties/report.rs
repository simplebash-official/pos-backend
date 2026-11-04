//! The warranty report (`GET /reports/warranties`): how often what the shop sells and repairs
//! comes back, how suppliers handle what is sent to them, and what free rework costs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How often one group of warranties was claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRateRow {
    /// The product, repair service or technician; left out only for jobs nobody was assigned
    /// to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Warranties given in the period.
    pub issued_count: u64,
    /// Of those, how many have had at least one claim.
    pub claimed_count: u64,
    /// `claimed / issued` in basis points (10 000 = every one).
    pub claim_rate_bps: i64,
}

/// What one supplier did with the claims sent to them in the period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierClaimRow {
    pub supplier_key: String,
    /// The supplier's name on the latest claim sent.
    pub supplier_name: String,
    pub sent_count: u64,
    /// Still with the supplier.
    pub away_count: u64,
    /// Still with the supplier past the day they gave.
    pub overdue_count: u64,
    pub repaired_count: u64,
    pub replaced_count: u64,
    pub credited_count: u64,
    pub rejected_count: u64,
    /// Average days from sending to getting it back; left out until one has come back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_days_away: Option<i64>,
}

/// Free rework in one shop-local month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReworkMonth {
    /// `YYYY-MM`.
    pub month: String,
    pub job_count: u64,
    /// Parts at cost.
    pub cost_cents: i64,
}

/// The warranty report for one period. A warranty counts in the period it was given; one that
/// was cancelled without ever being claimed (a voided or returned sale) and one that only
/// carries on a replaced unit's cover are left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub issued_count: u64,
    pub claimed_count: u64,
    pub claim_rate_bps: i64,
    /// Product warranties by product, most claimed first.
    pub by_product: Vec<ClaimRateRow>,
    /// Repair warranties by the work done (the item's description), most claimed first.
    pub by_repair_service: Vec<ClaimRateRow>,
    /// Repair warranties by the technician of the job, most claimed first.
    pub by_technician: Vec<ClaimRateRow>,
    /// Claims sent to a supplier in the period, by supplier, most sent first.
    pub by_supplier: Vec<SupplierClaimRow>,
    /// Rework jobs opened in the period, by month, oldest first.
    pub rework_by_month: Vec<ReworkMonth>,
    /// All rework jobs opened in the period.
    pub rework_job_count: u64,
    /// What they cost in parts.
    pub rework_cost_cents: i64,
}
