use super::formatted_date_time::formatted_date_time;
use super::paid_and_balance::paid_and_balance;
use super::shop_address_lines::shop_address_lines;
use super::shop_field::shop_field;
use super::shop_field_or_empty::shop_field_or_empty;
use crate::domain::billing::Invoice;

use serde_json::{Value, json};

/// Builds `data` for the "Thermal Receipt" template. `paper_width_mm` is the
/// terminal's configured receipt width (58 or 80) — see D9, the frontend
/// already switches between both, so this must too.
pub(crate) fn build_thermal_receipt_data(
    invoice: &Invoice,
    paper_width_mm: u32,
    paid_so_far_cents: i64,
) -> Value {
    let (amount_paid_cents, balance_due_cents) = paid_and_balance(invoice, paid_so_far_cents);
    let (formatted_date, formatted_time) = formatted_date_time(invoice);
    let shop = &invoice.shop_profile_snapshot;
    let warranty_text = invoice
        .warranty_terms_snapshot
        .clone()
        .unwrap_or_else(|| shop_field_or_empty(shop, "defaultWarrantyText").to_string());
    let footer_text = shop_field(shop, "receiptFooterText")
        .filter(|s| !s.is_empty())
        .unwrap_or("Thank you for your business!")
        .to_string();

    json!({
        "paperWidthMm": paper_width_mm,
        "invoiceNumber": invoice.invoice_number,
        "formattedDate": formatted_date,
        "formattedTime": formatted_time,
        "cashierName": invoice.cashier_name_snapshot,
        "customerName": invoice.customer_name_snapshot.clone().unwrap_or_default(),
        "customerPhone": invoice.customer_phone_snapshot.clone().unwrap_or_default(),
        "items": invoice.items,
        "subtotalCents": invoice.subtotal_cents,
        "discountType": invoice.discount_type,
        "discountValue": invoice.discount_value,
        "discountCents": invoice.discount_cents,
        "totalCents": invoice.total_cents,
        "paymentMethod": invoice.payment_method,
        "tenderedAmountCents": invoice.amount_received_cents.unwrap_or(invoice.total_cents),
        "changeDueCents": invoice.change_due_cents.unwrap_or(0),
        "cardLast4": invoice.card_last4.clone().unwrap_or_default(),
        "splitPayments": invoice.split_payments.clone().unwrap_or_default(),
        "isCredit": invoice.is_credit,
        "dueDate": invoice.due_date.clone().unwrap_or_default(),
        "amountPaidCents": amount_paid_cents,
        "balanceDueCents": balance_due_cents,
        "warrantyText": warranty_text,
        "footerText": footer_text,
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopLegalName": shop_field_or_empty(shop, "legalName"),
        "shopAddressLines": shop_address_lines(shop),
        "shopPrimaryPhone": shop_field_or_empty(shop, "primaryPhone"),
        "shopSecondaryPhone": shop_field_or_empty(shop, "secondaryPhone"),
    })
}
