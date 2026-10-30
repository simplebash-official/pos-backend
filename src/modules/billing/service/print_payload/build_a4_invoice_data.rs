use super::formatted_date_time::formatted_date_time;
use super::number_to_words_rupees::number_to_words_rupees;
use super::paid_and_balance::paid_and_balance;
use super::shop_address_lines::shop_address_lines;
use super::shop_bool::shop_bool;
use super::shop_field_or_empty::shop_field_or_empty;
use crate::domain::billing::Invoice;

use serde_json::{Value, json};

fn shop_number(shop: &Value, field: &str) -> Option<f64> {
    shop.get(field).and_then(Value::as_f64)
}

/// Builds `data` for the "A4 Invoice" template — field-for-field the
/// contract documented in that template's `.typ` header comment.
pub(crate) fn build_a4_invoice_data(
    invoice: &Invoice,
    copy_designation: &str,
    is_duplicate: bool,
    paid_so_far_cents: i64,
) -> Value {
    let (amount_paid_cents, balance_due_cents) = paid_and_balance(invoice, paid_so_far_cents);
    let (formatted_date, formatted_time) = formatted_date_time(invoice);
    let shop = &invoice.shop_profile_snapshot;
    let warranty_text = invoice
        .warranty_terms_snapshot
        .clone()
        .unwrap_or_else(|| shop_field_or_empty(shop, "defaultWarrantyText").to_string());

    let mut data = json!({
        // The shop logo, sent as a data: URI (the frontend's
        // `shopProfile.logoBase64`). document-server decodes it, stamps it on
        // this one render, and drops it — `""` means "use the built-in mark".
        "logoUrl": shop_field_or_empty(shop, "logoBase64"),
        "invoiceNumber": invoice.invoice_number,
        "formattedDate": formatted_date,
        "formattedTime": formatted_time,
        "dueDate": invoice.due_date.clone().unwrap_or_default(),
        "cashierName": invoice.cashier_name_snapshot,
        "status": invoice.status,
        "isCredit": invoice.is_credit,
        "copyDesignation": copy_designation,
        "isDuplicate": is_duplicate,
        "customerName": invoice.customer_name_snapshot.clone().unwrap_or_default(),
        "customerPhone": invoice.customer_phone_snapshot.clone().unwrap_or_default(),
        "customerAddress": invoice.customer_address_snapshot.clone().unwrap_or_default(),
        "paymentMethod": invoice.payment_method,
        "cardLast4": invoice.card_last4.clone().unwrap_or_default(),
        "tenderedAmountCents": invoice.amount_received_cents.unwrap_or(invoice.total_cents),
        "items": invoice.items,
        "subtotalCents": invoice.subtotal_cents,
        "discountType": invoice.discount_type,
        "discountValue": invoice.discount_value,
        "discountCents": invoice.discount_cents,
        "totalCents": invoice.total_cents,
        "vatRatePercent": shop_number(shop, "vatRate").map(|rate| rate * 100.0),
        "amountInWords": number_to_words_rupees(invoice.total_cents),
        "notes": invoice.notes.clone().unwrap_or_default(),
        "warrantyText": warranty_text,
        "showBankDetails": invoice.is_credit || invoice.payment_method == "online",
        "bankName": shop_field_or_empty(shop, "bankName"),
        "bankBranch": shop_field_or_empty(shop, "bankBranch"),
        "accountName": shop_field_or_empty(shop, "accountName"),
        "accountNumber": shop_field_or_empty(shop, "accountNumber"),
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopLegalName": shop_field_or_empty(shop, "legalName"),
        "shopEmail": shop_field_or_empty(shop, "email"),
        "shopWebsite": shop_field_or_empty(shop, "website"),
        "shopBusinessRegNo": shop_field_or_empty(shop, "businessRegNo"),
        "shopVatNo": shop_field_or_empty(shop, "vatNo"),
        "shopIsVatRegistered": shop_bool(shop, "isVatRegistered"),
        "shopAddressLines": shop_address_lines(shop),
        "shopPrimaryPhone": shop_field_or_empty(shop, "primaryPhone"),
        "shopSecondaryPhone": shop_field_or_empty(shop, "secondaryPhone"),
    });
    // Inserted after the fact: the `json!` above is already near the macro
    // recursion limit for this many keys.
    data["amountPaidCents"] = json!(amount_paid_cents);
    data["balanceDueCents"] = json!(balance_due_cents);
    // Change handed back for cash over the total, so the invoice can show
    // "amount received -> change -> total due" without re-deriving it.
    data["changeDueCents"] = json!(invoice.change_due_cents.unwrap_or(0));
    data
}
