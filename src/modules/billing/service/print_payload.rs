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

#[cfg(test)]
mod schema_drift_tests {
    use super::*;
    use crate::domain::billing::{CreditNote, Invoice, InvoiceStatus};
    use serde_json::json;

    /// Loads the schema sidecar the sibling document-server actually
    /// publishes for `template_name`. Returns `None` when that repository
    /// isn't checked out next to this one, so the suite stays runnable in a
    /// backend-only checkout — where it prints a notice instead of silently
    /// passing an empty guarantee.
    fn load_schema(template_name: &str) -> Option<serde_json::Value> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../document-server/templates/documents/"
        )
        .to_string()
            + template_name
            + ".schema.json";
        match std::fs::read_to_string(&path) {
            Ok(contents) => Some(serde_json::from_str(&contents).expect("schema sidecar parses")),
            Err(_) => {
                eprintln!("skipping schema drift check for {template_name}: no schema at {path}");
                None
            }
        }
    }

    /// Panics with every violation listed when `payload` doesn't satisfy the
    /// published contract — the same check `DocumentServerClient::render`
    /// enforces at request time, run here against the *real* sidecar files.
    fn assert_matches_schema(template_name: &str, payload: &serde_json::Value) {
        let Some(schema) = load_schema(template_name) else {
            return;
        };
        let validator = jsonschema::validator_for(&schema).expect("published schema compiles");
        let violations: Vec<String> = validator
            .iter_errors(payload)
            .map(|err| {
                let path = err.instance_path().to_string();
                if path.is_empty() {
                    err.to_string()
                } else {
                    format!("at '{path}': {err}")
                }
            })
            .collect();
        assert!(
            violations.is_empty(),
            "{template_name} payload drifted from its published data schema:\n  {}",
            violations.join("\n  ")
        );
    }

    fn cash_sale_invoice() -> Invoice {
        serde_json::from_value(json!({
            "id": "64b0000000000000000000a1",
            "key": "inv_testcash",
            "invoiceNumber": "INV-000001",
            "customerKey": "cust_test",
            "customerNameSnapshot": "Kasun Silva",
            "customerPhoneSnapshot": "077 123 4567",
            "customerAddressSnapshot": "45 Lake Road, Colombo 06",
            "cashierId": "usr_admin",
            "cashierNameSnapshot": "Default Admin",
            "items": [
                {
                    "productKey": "prod_test",
                    "name": "Screen Protector - iPhone 14",
                    "sku": "ACC-SP-014",
                    "unitPriceCents": 15000,
                    "quantity": 2,
                    "discountCents": 0,
                    "totalCents": 30000,
                    "sourceType": "retail"
                },
                {
                    "name": "Screen Replacement",
                    "unitPriceCents": 85000,
                    "quantity": 1,
                    "discountCents": 5000,
                    "totalCents": 80000,
                    "sourceType": "repair",
                    "sourceTicketKey": "rep_test",
                    "sourceTicketNumber": "REP-000001",
                    "assignedEmployeeName": "Ruwan Fernando"
                }
            ],
            "subtotalCents": 110000,
            "discountType": "percentage",
            "discountValue": 5.0,
            "discountCents": 5500,
            "totalCents": 104500,
            "paymentMethod": "cash",
            "isCredit": false,
            "amountReceivedCents": 110000,
            "changeDueCents": 5500,
            "status": "paid",
            "isOverdue": false,
            "shopProfileSnapshot": {
                "tradingName": "TechFix Repairs",
                "legalName": "TechFix Repairs (Pvt) Ltd",
                "primaryPhone": "011 234 5678",
                "secondaryPhone": "",
                "addressLines": ["123 Galle Road", "Colombo 04"],
                "vatRate": 0.0,
                "receiptFooterText": "Thank you!"
            },
            "createdAt": "2026-08-23T09:00:00Z",
            "updatedAt": "2026-08-23T09:00:00Z"
        }))
        .expect("cash-sale invoice fixture deserializes")
    }

    #[test]
    fn thermal_receipt_payload_matches_published_schema() {
        let invoice = cash_sale_invoice();
        let payload = build_thermal_receipt_data(&invoice, 80);
        assert_matches_schema("thermal-receipt", &payload);
    }

    #[test]
    fn a4_invoice_payload_matches_published_schema() {
        let invoice = cash_sale_invoice();
        let payload = build_a4_invoice_data(&invoice, "ORIGINAL — CUSTOMER COPY", false);
        assert_matches_schema("a4-invoice", &payload);
    }

    /// Every lifecycle status must pass the published enum — the schema's
    /// `status` enum drifted once (`partially_paid`/`voided`/`closed` were
    /// rejected), which only surfaced as a customer-facing 422 on the real
    /// document-server because tests used a mock with minimal schemas.
    #[test]
    fn a4_invoice_covers_every_lifecycle_status() {
        for status in [
            InvoiceStatus::Paid,
            InvoiceStatus::Pending,
            InvoiceStatus::PartiallyPaid,
            InvoiceStatus::Voided,
            InvoiceStatus::Closed,
        ] {
            let mut invoice = cash_sale_invoice();
            invoice.status = status;
            let payload = build_a4_invoice_data(&invoice, "DUPLICATE COPY", true);
            assert_matches_schema("a4-invoice", &payload);
        }
    }

    /// The nastiest drift class: fields that serialize to JSON `null`
    /// (`vatRatePercent` when the shop snapshot carries no VAT rate) or come
    /// from an empty snapshot entirely — a minimal shop profile must still
    /// produce a schema-valid payload for both receipt layouts.
    #[test]
    fn split_payment_credit_invoice_with_empty_shop_matches_schemas() {
        let mut invoice = cash_sale_invoice();
        invoice.status = InvoiceStatus::PartiallyPaid;
        invoice.payment_method = "split".to_string();
        invoice.is_credit = true;
        invoice.due_date = Some("2026-09-06".to_string());
        invoice.amount_received_cents = Some(50000);
        invoice.change_due_cents = None;
        invoice.split_payments = Some(vec![
            serde_json::from_value(json!({
                "method": "cash", "amountCents": 50000
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "method": "card", "amountCents": 54500, "cardLast4": "4242"
            }))
            .unwrap(),
        ]);
        // No VAT rate key → builder emits `vatRatePercent: null`; no shop
        // fields at all beyond what `shop_field_or_empty` can fall back on.
        invoice.shop_profile_snapshot = json!({});

        let thermal = build_thermal_receipt_data(&invoice, 58);
        assert_matches_schema("thermal-receipt", &thermal);

        let a4 = build_a4_invoice_data(&invoice, "ORIGINAL — CUSTOMER COPY", false);
        assert_matches_schema("a4-invoice", &a4);
    }

    fn base_credit_note() -> CreditNote {
        serde_json::from_value(json!({
            "id": "64b0000000000000000000b1",
            "key": "cn_test",
            "creditNoteNumber": "CN-000001",
            "invoiceKey": "inv_testcash",
            "invoiceNumber": "INV-000001",
            "noReceipt": false,
            "customerKey": "cust_test",
            "customerNameSnapshot": "Kasun Silva",
            "cashierId": "usr_admin",
            "cashierNameSnapshot": "Default Admin",
            "returnedItems": [
                {
                    "productKey": "prod_test",
                    "name": "Screen Protector - iPhone 14",
                    "sku": "ACC-SP-014",
                    "quantity": 1,
                    "unitPriceCents": 15000,
                    "totalCents": 15000,
                    "reason": "defective",
                    "condition": "damaged",
                    "disposition": "write_off_scrap"
                }
            ],
            "returnSubtotalCents": 15000,
            "exchangeSubtotalCents": 0,
            "netRefundCents": 15000,
            "refundCashCents": 10000,
            "balanceReductionCents": 5000,
            "refundBreakdown": [
                {"method": "cash", "amountCents": 10000}
            ],
            "status": "resolved",
            "isManagerOverride": false,
            "shopProfileSnapshot": {},
            "createdAt": "2026-08-23T10:00:00Z",
            "updatedAt": "2026-08-23T10:00:00Z"
        }))
        .expect("credit note fixture deserializes")
    }

    #[test]
    fn credit_note_payloads_match_published_schema() {
        let invoice = cash_sale_invoice();

        let linked = build_credit_note_data(&base_credit_note(), Some(&invoice));
        assert_matches_schema("credit-note", &linked);

        // No-receipt variant: no shop profile exists, so every shop field
        // falls back to empty — must still satisfy the required strings.
        let mut no_receipt = base_credit_note();
        no_receipt.no_receipt = true;
        no_receipt.invoice_key = None;
        no_receipt.invoice_number = None;
        let payload = build_credit_note_data(&no_receipt, None);
        assert_matches_schema("credit-note", &payload);
    }
}
