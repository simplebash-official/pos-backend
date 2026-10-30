use super::cash_movement::CashMovement;
use super::register_session::RegisterSession;
use super::z_report_cash_reconciliation::ZReportCashReconciliation;
use super::z_report_movement_group::ZReportMovementGroup;
use super::z_report_payment_method_summary::ZReportPaymentMethodSummary;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// Full Z-Report (End-of-Session Summary).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportResponse {
    pub session: RegisterSession,
    pub gross_sales_cents: i64,
    pub returns_cents: i64,
    pub net_sales_cents: i64,
    pub discount_cents: i64,
    pub total_invoices_count: u64,
    pub payment_methods: Vec<ZReportPaymentMethodSummary>,
    pub cash_movements: Vec<CashMovement>,
    /// The shift's drawer movements totalled by kind and category, in the order the kinds are picked.
    pub movement_summary: Vec<ZReportMovementGroup>,
    pub cash_reconciliation: ZReportCashReconciliation,
    pub generated_at: DateTime<Utc>,
}
