// Builds the JSON payload handed to document-server's Typst templates
// (`a4-invoice.typ` / `thermal-receipt.typ`) from a persisted `Invoice`.
// Rust port of the frontend's `buildPrintPayload.ts` — same field
// contract, called by `modules::documents::service::get_or_render` via
// `modules::billing::routes`'s document-fetch handlers (not written yet
// here; this module only builds the data, `documents::service` renders it).

use serde_json::{Value, json};

use crate::domain::billing::{CreditNote, Invoice, ItemCondition, ItemDisposition};

/// `invoice.shop_profile_snapshot` is the frontend's `ShopProfile` object,
/// embedded verbatim at sale-completion time (D2) — this just reads fields
/// off it by name by way of `serde_json::Value` indexing, since the
/// snapshot's shape is owned by the frontend, not modeled as a Rust struct
/// here (no backend shop-profile module exists, deliberately — see D2).
fn shop_field<'a>(shop: &'a Value, field: &str) -> Option<&'a str> {
    shop.get(field).and_then(Value::as_str)
}

fn shop_field_or_empty<'a>(shop: &'a Value, field: &str) -> &'a str {
    shop_field(shop, field).unwrap_or("")
}

fn shop_address_lines(shop: &Value) -> Vec<&str> {
    shop.get("addressLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn shop_bool(shop: &Value, field: &str) -> bool {
    shop.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn shop_number(shop: &Value, field: &str) -> Option<f64> {
    shop.get(field).and_then(Value::as_f64)
}

fn formatted_date_time(invoice: &Invoice) -> (String, String) {
    let formatted_date = invoice.created_at.format("%d %b %Y").to_string();
    let formatted_time = invoice.created_at.format("%H:%M").to_string();
    (formatted_date, formatted_time)
}

/// Builds `data` for `a4-invoice.typ` — field-for-field the contract
/// documented in that template's header comment.
pub(crate) fn build_a4_invoice_data(
    invoice: &Invoice,
    copy_designation: &str,
    is_duplicate: bool,
) -> Value {
    let (formatted_date, formatted_time) = formatted_date_time(invoice);
    let shop = &invoice.shop_profile_snapshot;
    let warranty_text = invoice
        .warranty_terms_snapshot
        .clone()
        .unwrap_or_else(|| shop_field_or_empty(shop, "defaultWarrantyText").to_string());

    json!({
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
    })
}

/// Builds `data` for `thermal-receipt.typ`. `paper_width_mm` is the
/// terminal's configured receipt width (58 or 80) — see D9, the frontend
/// already switches between both, so this must too.
pub(crate) fn build_thermal_receipt_data(invoice: &Invoice, paper_width_mm: u32) -> Value {
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
        "warrantyText": warranty_text,
        "footerText": footer_text,
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopLegalName": shop_field_or_empty(shop, "legalName"),
        "shopAddressLines": shop_address_lines(shop),
        "shopPrimaryPhone": shop_field_or_empty(shop, "primaryPhone"),
        "shopSecondaryPhone": shop_field_or_empty(shop, "secondaryPhone"),
    })
}

/// Plain-language label for an `ItemCondition`, matching the frontend's
/// `invoiceStatus.ts` color/label table exactly — the printed slip must read
/// the same words a cashier sees on screen, not the raw enum spelling.
fn condition_label(condition: ItemCondition) -> &'static str {
    match condition {
        ItemCondition::Resalable => "Good — Resalable",
        ItemCondition::Damaged => "Damaged / Faulty",
        ItemCondition::OpenBoxDiscount => "Open Box — Discounted Resale",
        ItemCondition::PendingInspection => "Pending Inspection",
    }
}

/// Plain-language label for an `ItemDisposition` — empty string when the
/// line has none (`condition` other than `Damaged`), matching the
/// document-server template's expectation of an always-present string field.
fn disposition_label(disposition: Option<ItemDisposition>) -> &'static str {
    match disposition {
        Some(ItemDisposition::ReturnToSupplier) => "Return to Supplier",
        Some(ItemDisposition::WriteOffScrap) => "Write Off",
        Some(ItemDisposition::RepairPending) => "Repair Pending",
        None => "",
    }
}

/// Builds `data` for `credit-note.typ`. `invoice` is `None` for a
/// no-receipt credit note (see `CreditNote.no_receipt`) — there is no shop
/// profile snapshot to read in that case (only an `Invoice` carries one, per
/// D2), so shop/warranty fields fall back to an empty JSON object, matching
/// this module's existing `shop_field_or_empty` graceful-degradation
/// pattern rather than fabricating placeholder shop details.
pub(crate) fn build_credit_note_data(credit_note: &CreditNote, invoice: Option<&Invoice>) -> Value {
    let empty_shop = json!({});
    let shop = invoice.map_or(&empty_shop, |inv| &inv.shop_profile_snapshot);
    let formatted_date = credit_note.created_at.format("%d %b %Y").to_string();
    let cashier_name = credit_note.cashier_name_snapshot.clone();
    let customer_name = credit_note
        .customer_name_snapshot
        .clone()
        .unwrap_or_else(|| {
            invoice
                .and_then(|inv| inv.customer_name_snapshot.clone())
                .unwrap_or_default()
        });
    let customer_phone = invoice
        .and_then(|inv| inv.customer_phone_snapshot.clone())
        .unwrap_or_default();
    let original_invoice_number = if credit_note.no_receipt {
        "No Receipt Provided".to_string()
    } else {
        credit_note.invoice_number.clone().unwrap_or_default()
    };

    let items: Vec<Value> = credit_note
        .returned_items
        .iter()
        .map(|item| {
            json!({
                "name": item.name,
                "sku": item.sku.clone().unwrap_or_default(),
                "serialNumber": item.serial_number.clone().unwrap_or_default(),
                "quantity": item.quantity,
                "condition": condition_label(item.condition),
                "disposition": disposition_label(item.disposition),
                "unitPriceCents": item.unit_price_cents,
                "totalCents": item.total_cents,
            })
        })
        .collect();

    json!({
        "creditNoteNumber": credit_note.credit_note_number,
        "formattedDate": formatted_date,
        "cashierName": cashier_name,
        "originalInvoiceNumber": original_invoice_number,
        "noReceipt": credit_note.no_receipt,
        "isManagerOverride": credit_note.is_manager_override,
        "exchangeReference": credit_note.exchange_reference.clone().unwrap_or_default(),
        "customerName": customer_name,
        "customerPhone": customer_phone,
        "items": items,
        "refundCashCents": credit_note.refund_cash_cents,
        "balanceReductionCents": credit_note.balance_reduction_cents,
        "refundBreakdown": credit_note.refund_breakdown,
        "notes": credit_note.notes.clone().unwrap_or_default(),
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
    })
}

/// Rust port of the frontend's `numberToWordsRupees` (`src/shared/lib/
/// numberToWords.ts`) — same lakh/thousand (Indian numbering system)
/// wording, so an invoice's printed amount-in-words is identical whichever
/// side renders it. `CURRENCY.decimals = 0` app-wide (see `formatMoney`),
/// but this mirrors the frontend's cents-aware wording exactly rather than
/// assuming whole rupees, since the source of truth is the TS function, not
/// the currency's display rounding.
pub(crate) fn number_to_words_rupees(amount_cents: i64) -> String {
    if amount_cents <= 0 {
        return "Sri Lankan Rupees Zero Only".to_string();
    }

    let total_rupees = amount_cents / 100;
    let cents = amount_cents % 100;

    let rupee_text = convert_number(total_rupees);
    let mut result = format!("Sri Lankan Rupees {rupee_text}");

    if cents > 0 {
        let cents_text = convert_below_thousand(cents);
        result.push_str(&format!(" and Cents {cents_text}"));
    }

    result.push_str(" Only");
    result
}

const ONES: [&str; 20] = [
    "",
    "One",
    "Two",
    "Three",
    "Four",
    "Five",
    "Six",
    "Seven",
    "Eight",
    "Nine",
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];

const TENS: [&str; 10] = [
    "", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];

fn convert_below_thousand(n: i64) -> String {
    if n == 0 {
        return String::new();
    }
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    if n < 100 {
        let rem = n % 10;
        let tens_word = TENS[(n / 10) as usize];
        return if rem > 0 {
            format!("{tens_word} {}", ONES[rem as usize])
        } else {
            tens_word.to_string()
        };
    }
    let hundred_rem = n % 100;
    let hundreds_word = ONES[(n / 100) as usize];
    if hundred_rem > 0 {
        format!(
            "{hundreds_word} Hundred {}",
            convert_below_thousand(hundred_rem)
        )
    } else {
        format!("{hundreds_word} Hundred")
    }
}

fn convert_number(n: i64) -> String {
    if n == 0 {
        return "Zero".to_string();
    }

    let lakh = n / 100_000;
    let mut remainder = n % 100_000;
    let thousand = remainder / 1000;
    remainder %= 1000;

    let mut parts: Vec<String> = Vec::new();
    if lakh > 0 {
        parts.push(format!("{} Lakh", convert_below_thousand(lakh)));
    }
    if thousand > 0 {
        parts.push(format!("{} Thousand", convert_below_thousand(thousand)));
    }
    if remainder > 0 {
        parts.push(convert_below_thousand(remainder));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_frontend_examples() {
        assert_eq!(
            number_to_words_rupees(8_000_000),
            "Sri Lankan Rupees Eighty Thousand Only"
        );
        assert_eq!(
            number_to_words_rupees(8_000_050),
            "Sri Lankan Rupees Eighty Thousand and Cents Fifty Only"
        );
        assert_eq!(number_to_words_rupees(0), "Sri Lankan Rupees Zero Only");
        assert_eq!(
            number_to_words_rupees(1_100_000),
            "Sri Lankan Rupees Eleven Thousand Only"
        );
    }
}
