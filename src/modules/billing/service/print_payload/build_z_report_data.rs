use super::shop_address_lines::shop_address_lines;
use super::shop_bool::shop_bool;
use super::shop_field::shop_field;
use super::shop_field_or_empty::shop_field_or_empty;

use crate::domain::sessions::ZReportResponse;
use serde_json::{Value, json};

/// Builds `data` for the "Z-Report" template — field-for-field the contract
/// documented in `doc_temp_ZRep0rtV1Std000.schema.json`.
pub(crate) fn build_z_report_data(report: &ZReportResponse, shop: &Value) -> Value {
    let session = &report.session;
    let opened_at = session.opened_at.format("%d %b %Y %H:%M").to_string();
    let closed_at = session
        .closed_at
        .map(|dt| dt.format("%d %b %Y %H:%M").to_string())
        .unwrap_or_default();
    let generated_at = report.generated_at.format("%d %b %Y %H:%M").to_string();

    let payment_methods: Vec<Value> = report
        .payment_methods
        .iter()
        .map(|pm| {
            json!({
                "method": pm.method,
                "count": pm.count,
                "totalCents": pm.total_cents,
            })
        })
        .collect();

    let recon = &report.cash_reconciliation;
    let cash_reconciliation = json!({
        "openingFloatCents": recon.opening_float_cents,
        "cashSalesCents": recon.cash_sales_cents,
        "payInCents": recon.pay_in_cents,
        "payOutCents": recon.pay_out_cents,
        "cashRefundsCents": recon.cash_refunds_cents,
        "ownerWithdrawalsCents": recon.owner_withdrawals_cents,
        "cashDropsCents": recon.cash_drops_cents,
        "expectedCashCents": recon.expected_cash_cents,
        "countedCashCents": recon.counted_cash_cents,
        "varianceCents": recon.variance_cents,
        "closingFloatCents": recon.closing_float_cents,
        "safeDepositCents": recon.safe_deposit_cents,
    });

    let cash_movements: Vec<Value> = report
        .cash_movements
        .iter()
        .map(|cm| {
            json!({
                "movementNumber": cm.movement_number,
                "movementType": cm.movement_type.as_str(),
                "amountCents": cm.amount_cents,
                // The published Z-report schema still requires these two lines; `movementSummary` below carries the grouped totals.
                "reason": cm.custom_label.clone().unwrap_or_else(|| cm.category_name.clone()),
                "recipientOrSource": cm.payee_name.clone().unwrap_or_default(),
                "recordedByNameSnapshot": cm.recorded_by_name_snapshot,
                "createdAt": cm.created_at.format("%d %b %Y %H:%M").to_string(),
            })
        })
        .collect();

    let movement_summary: Vec<Value> = report
        .movement_summary
        .iter()
        .map(|g| {
            json!({
                "movementType": g.movement_type.as_str(),
                "direction": g.direction.as_str(),
                "categoryName": g.category_name,
                "count": g.count,
                "totalCents": g.total_cents,
            })
        })
        .collect();

    let mut data = json!({
        "logoUrl": shop_field_or_empty(shop, "logoBase64"),
        "sessionNumber": session.session_number,
        "terminalId": session.terminal_id,
        "cashierName": session.cashier_name_snapshot,
        "status": session.status.as_str(),
        "openedAt": opened_at,
        "closedAt": closed_at,
        "grossSalesCents": report.gross_sales_cents,
        "returnsCents": report.returns_cents,
        "netSalesCents": report.net_sales_cents,
        "discountCents": report.discount_cents,
        "totalInvoicesCount": report.total_invoices_count,
        "paymentMethods": payment_methods,
        "cashReconciliation": cash_reconciliation,
        "cashMovements": cash_movements,
        "movementSummary": movement_summary,
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopAddressLines": shop_address_lines(shop),
        "generatedAt": generated_at,
    });

    if let Some(ref closing_cashier) = session.closing_cashier_name_snapshot {
        data["closingCashierName"] = json!(closing_cashier);
    }
    if let Some(ref note) = session.opening_note {
        data["openingNote"] = json!(note);
    }
    if let Some(ref note) = session.closing_note {
        data["closingNote"] = json!(note);
    }
    if let Some(val) = shop_field(shop, "legalName") {
        data["shopLegalName"] = json!(val);
    }
    if let Some(val) = shop_field(shop, "email") {
        data["shopEmail"] = json!(val);
    }
    if let Some(val) = shop_field(shop, "website") {
        data["shopWebsite"] = json!(val);
    }
    if let Some(val) = shop_field(shop, "businessRegNo") {
        data["shopBusinessRegNo"] = json!(val);
    }
    if let Some(val) = shop_field(shop, "vatNo") {
        data["shopVatNo"] = json!(val);
    }
    data["shopIsVatRegistered"] = json!(shop_bool(shop, "isVatRegistered"));
    if let Some(val) = shop_field(shop, "primaryPhone") {
        data["shopPrimaryPhone"] = json!(val);
    }
    if let Some(val) = shop_field(shop, "secondaryPhone") {
        data["shopSecondaryPhone"] = json!(val);
    }

    data
}
