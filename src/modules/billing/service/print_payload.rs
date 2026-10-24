// Builds the JSON payload handed to document-server's Typst templates (the
// "A4 Invoice" / "Thermal Receipt" / "Credit Note" templates, addressed by
// the slugs `a4-invoice` / `thermal-receipt` / `credit-note` — see
// `clients::document_server`) from a persisted `Invoice`. Rust port of the
// frontend's `buildPrintPayload.ts` — same field contract, called by
// `modules::documents::service::get_or_render` via `modules::billing::routes`'s
// document-fetch handlers; this module only builds the data, `documents::service`
// renders it.

use serde_json::{Value, json};

use crate::domain::{
    billing::{CreditNote, Invoice, ItemCondition, ItemDisposition},
    quotations::Quotation,
    sessions::ZReportResponse,
};

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

/// How much has been paid against this invoice, and how much is still owed,
/// for the printed "Amount Paid" / "Balance Due" lines. `paid_so_far_cents`
/// is the sum of every payment row (checkout deposit + later installments);
/// callers that don't have it can pass `invoice.amount_received_cents`.
fn paid_and_balance(invoice: &Invoice, paid_so_far_cents: i64) -> (i64, i64) {
    if invoice.is_credit {
        let balance = (invoice.total_cents - paid_so_far_cents - invoice.refunded_cents).max(0);
        (paid_so_far_cents, balance)
    } else {
        (invoice.total_cents, 0)
    }
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

/// Builds `data` for the "Credit Note" template. `invoice` is `None` for a
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

/// Builds `data` for the "Quotation" template. `shop` is the shop profile
/// frozen on the quotation when it was made (as a JSON object with the same
/// keys the invoice snapshot uses), kept out of `Quotation` itself because it
/// can carry a base64 logo. A quotation is an offer, not a bill: there is no
/// payment, cashier, tax or status-of-payment in the contract.
pub(crate) fn build_quotation_data(quotation: &Quotation, shop: &Value) -> Value {
    let long_date = |d: chrono::NaiveDate| d.format("%d %b %Y").to_string();
    // A stored `valid_until` is validated as YYYY-MM-DD on create; if it ever
    // is not, print it as stored rather than fail the whole print.
    let valid_until = chrono::NaiveDate::parse_from_str(&quotation.valid_until, "%Y-%m-%d")
        .map(long_date)
        .unwrap_or_else(|_| quotation.valid_until.clone());

    let mut data = json!({
        "logoUrl": shop_field_or_empty(shop, "logoBase64"),
        "quotationNumber": quotation.quotation_number,
        "formattedDate": quotation.created_at.format("%d %b %Y").to_string(),
        "validUntil": valid_until,
        "isCancelled": quotation.status == crate::domain::quotations::STATUS_CANCELLED,
        "customerName": quotation.customer_name,
        "customerPhone": quotation.customer_phone.clone().unwrap_or_default(),
        "customerAddress": quotation.customer_address.clone().unwrap_or_default(),
        "items": quotation.items,
        "subtotalCents": quotation.subtotal_cents,
        "discountType": quotation.discount_type,
        "discountValue": quotation.discount_value,
        "discountCents": quotation.discount_cents,
        "totalCents": quotation.total_cents,
        "amountInWords": number_to_words_rupees(quotation.total_cents),
        "notes": quotation.notes.clone().unwrap_or_default(),
        "terms": quotation.terms.clone().unwrap_or_default(),
        "preparedBy": quotation.prepared_by.clone().unwrap_or_default(),
        "footerText": shop_field_or_empty(shop, "defaultFooterText"),
        "shopTradingName": shop_field_or_empty(shop, "tradingName"),
        "shopLegalName": shop_field_or_empty(shop, "legalName"),
        "shopEmail": shop_field_or_empty(shop, "email"),
        "shopWebsite": shop_field_or_empty(shop, "website"),
        "shopBusinessRegNo": shop_field_or_empty(shop, "businessRegNo"),
        "shopAddressLines": shop_address_lines(shop),
        "shopPrimaryPhone": shop_field_or_empty(shop, "primaryPhone"),
        "shopSecondaryPhone": shop_field_or_empty(shop, "secondaryPhone"),
    });
    // Inserted after the fact: the `json!` above is near the macro recursion limit.
    data["bankName"] = json!(shop_field_or_empty(shop, "bankName"));
    data["bankBranch"] = json!(shop_field_or_empty(shop, "bankBranch"));
    data["accountName"] = json!(shop_field_or_empty(shop, "accountName"));
    data["accountNumber"] = json!(shop_field_or_empty(shop, "accountNumber"));
    data
}

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

    /// Every `<name>.schema.json` the sibling document-server publishes for a
    /// *document kind* (`x-document-kind` in the schema), as `(title, schema)`.
    /// The payload builders feed **all** templates of a kind, so each one must
    /// be checked, not just the first. Returns an empty list (with a notice)
    /// when that repository isn't checked out next to this one, so the suite
    /// stays runnable in a backend-only checkout — where it prints a notice
    /// instead of silently passing an empty guarantee.
    fn load_schemas_of_kind(kind: &str) -> Vec<(String, serde_json::Value)> {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../document-server/templates/documents"
        );
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!(
                "skipping schema drift check for {kind}: no document-server checkout at {dir}"
            );
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(".schema.json") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(schema) = serde_json::from_str::<serde_json::Value>(&contents) else {
                continue;
            };
            if schema.get("x-document-kind").and_then(|k| k.as_str()) == Some(kind) {
                let title = schema
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("(untitled)")
                    .to_string();
                found.push((title, schema));
            }
        }
        if found.is_empty() {
            panic!("no template in {dir} declares x-document-kind {kind:?}");
        }
        found
    }

    /// Panics with every violation listed when `payload` doesn't satisfy the
    /// published contract of **any** template of this kind — the same check
    /// `DocumentServerClient::render_template` enforces at request time, run
    /// here against the *real* sidecar files.
    fn assert_matches_schema(kind: &str, payload: &serde_json::Value) {
        for (title, schema) in load_schemas_of_kind(kind) {
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
                "{kind} payload drifted from the published data schema of {title:?}:\n  {}",
                violations.join("\n  ")
            );
        }
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
        let payload =
            build_thermal_receipt_data(&invoice, 80, invoice.amount_received_cents.unwrap_or(0));
        assert_matches_schema("thermal-receipt", &payload);
    }

    #[test]
    fn a4_invoice_payload_matches_published_schema() {
        let invoice = cash_sale_invoice();
        let payload = build_a4_invoice_data(
            &invoice,
            "ORIGINAL — CUSTOMER COPY",
            false,
            invoice.amount_received_cents.unwrap_or(0),
        );
        assert_matches_schema("a4-invoice", &payload);
    }

    // The invoice shows "amount received -> change -> total due", so the A4 payload
    // must carry all four numbers and a paid invoice must owe nothing.
    #[test]
    fn a4_invoice_payload_carries_amount_given_change_and_what_is_still_owed() {
        let invoice = cash_sale_invoice();
        let payload = build_a4_invoice_data(&invoice, "ORIGINAL", false, 104_500);
        assert_eq!(payload["totalCents"], 104_500);
        assert_eq!(payload["tenderedAmountCents"], 110_000);
        assert_eq!(payload["changeDueCents"], 5_500);
        assert_eq!(payload["amountPaidCents"], 104_500);
        assert_eq!(payload["balanceDueCents"], 0);
        assert_matches_schema("a4-invoice", &payload);
    }

    #[test]
    fn a4_credit_invoice_owes_the_balance_and_gives_no_change() {
        let mut invoice = cash_sale_invoice();
        invoice.is_credit = true;
        invoice.amount_received_cents = Some(40_000);
        invoice.change_due_cents = None;
        let payload = build_a4_invoice_data(&invoice, "ORIGINAL", false, 40_000);
        assert_eq!(payload["tenderedAmountCents"], 40_000);
        assert_eq!(payload["changeDueCents"], 0);
        assert_eq!(payload["amountPaidCents"], 40_000);
        assert_eq!(payload["balanceDueCents"], 64_500);
        assert_matches_schema("a4-invoice", &payload);
    }

    #[test]
    fn a4_invoice_carries_the_shop_logo_as_logo_url() {
        // No `logoBase64` in the snapshot -> empty string (the "no logo"
        // signal document-server understands), still schema-valid.
        let mut invoice = cash_sale_invoice();
        let payload = build_a4_invoice_data(
            &invoice,
            "ORIGINAL — CUSTOMER COPY",
            false,
            invoice.amount_received_cents.unwrap_or(0),
        );
        assert_eq!(payload["logoUrl"], "");
        assert_matches_schema("a4-invoice", &payload);

        // A shop logo (a data: URI) is passed straight through as `logoUrl`.
        let logo = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        invoice.shop_profile_snapshot["logoBase64"] = json!(logo);
        let payload = build_a4_invoice_data(
            &invoice,
            "ORIGINAL — CUSTOMER COPY",
            false,
            invoice.amount_received_cents.unwrap_or(0),
        );
        assert_eq!(payload["logoUrl"], logo);
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
            let payload = build_a4_invoice_data(
                &invoice,
                "DUPLICATE COPY",
                true,
                invoice.amount_received_cents.unwrap_or(0),
            );
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

        let thermal =
            build_thermal_receipt_data(&invoice, 58, invoice.amount_received_cents.unwrap_or(0));
        assert_matches_schema("thermal-receipt", &thermal);

        let a4 = build_a4_invoice_data(
            &invoice,
            "ORIGINAL — CUSTOMER COPY",
            false,
            invoice.amount_received_cents.unwrap_or(0),
        );
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

    fn quotation(status: &str, discount_cents: i64) -> crate::domain::quotations::Quotation {
        use crate::domain::quotations::{Quotation, QuotationItem};
        Quotation {
            id: "64b0000000000000000000b1".to_string(),
            key: "quo_test".to_string(),
            quotation_number: "QUO-0001".to_string(),
            status: status.to_string(),
            is_expired: false,
            customer_key: None,
            customer_name: "Nimal Perera".to_string(),
            customer_phone: Some("077 123 4567".to_string()),
            customer_address: None,
            items: vec![QuotationItem {
                product_key: None,
                name: "Screen replacement".to_string(),
                sku: None,
                unit_price_cents: 10_000,
                quantity: 2,
                discount_cents: 0,
                total_cents: 20_000,
            }],
            subtotal_cents: 20_000,
            discount_type: "fixed".to_string(),
            discount_value: discount_cents as f64,
            discount_cents,
            total_cents: 20_000 - discount_cents,
            valid_until: "2030-01-31".to_string(),
            notes: None,
            terms: Some("50% advance.".to_string()),
            prepared_by: Some("Kamal".to_string()),
            created_at: "2026-10-13T09:00:00Z".parse().unwrap(),
            updated_at: "2026-10-13T09:00:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn quotation_payload_matches_published_schema() {
        let shop = cash_sale_invoice().shop_profile_snapshot;
        assert_matches_schema(
            "quotation",
            &build_quotation_data(&quotation("active", 0), &shop),
        );
        // Cancelled, discounted, and printed for a shop that never filled in
        // its profile (every shop field empty): still satisfies the contract.
        assert_matches_schema(
            "quotation",
            &build_quotation_data(&quotation("cancelled", 500), &json!({})),
        );
    }

    #[test]
    fn quotation_payload_carries_dates_cancellation_and_words() {
        let shop = json!({ "tradingName": "TechFix", "logoBase64": "data:image/png;base64,AAAA" });
        let active = build_quotation_data(&quotation("active", 0), &shop);
        assert_eq!(active["quotationNumber"], "QUO-0001");
        assert_eq!(active["formattedDate"], "13 Oct 2026");
        assert_eq!(active["validUntil"], "31 Jan 2030");
        assert_eq!(active["isCancelled"], false);
        assert_eq!(active["logoUrl"], "data:image/png;base64,AAAA");
        assert_eq!(active["shopTradingName"], "TechFix");
        assert_eq!(active["totalCents"], 20_000);
        assert_eq!(
            active["amountInWords"],
            "Sri Lankan Rupees Two Hundred Only"
        );
        // A quotation is an offer: none of the invoice-only fields leak in.
        for invoice_only in ["cashierName", "paymentMethod", "balanceDueCents", "status"] {
            assert!(active.get(invoice_only).is_none(), "{invoice_only}");
        }

        let cancelled = build_quotation_data(&quotation("cancelled", 0), &shop);
        assert_eq!(cancelled["isCancelled"], true);
    }

    fn base_z_report() -> ZReportResponse {
        use crate::domain::sessions::{
            CashDirection, CashMovement, CashMovementType, RegisterSession, SessionStatus,
            ZReportCashReconciliation, ZReportPaymentMethodSummary, ZReportResponse,
        };
        ZReportResponse {
            session: RegisterSession {
                id: "64b0000000000000000000c1".to_string(),
                key: "ses_test".to_string(),
                session_number: "SES-000042".to_string(),
                terminal_id: "TERM-01".to_string(),
                cashier_id: "usr_admin".to_string(),
                cashier_name_snapshot: "Kasun Perera".to_string(),
                status: SessionStatus::Closed,
                opened_at: "2026-10-21T08:30:00Z".parse().unwrap(),
                opening_float_cents: 200_000,
                opening_denominations: None,
                opening_note: Some("Morning opening".to_string()),
                opening_discrepancy_cents: 0,
                opening_discrepancy_reason: None,
                opening_approved_by_id: None,
                opening_approved_by_name: None,
                closed_at: Some("2026-10-21T18:00:00Z".parse().unwrap()),
                closing_cashier_id: Some("usr_admin".to_string()),
                closing_cashier_name_snapshot: Some("Kasun Perera".to_string()),
                expected_cash_cents: Some(520_000),
                counted_cash_cents: Some(520_000),
                cash_variance_cents: Some(0),
                closing_float_cents: Some(200_000),
                cash_drop_to_safe_cents: Some(320_000),
                closing_denominations: None,
                closing_note: Some("Reconciled perfectly".to_string()),
                variance_reason: None,
                variance_approved_by_id: None,
                variance_approved_by_name: None,
                reopened_at: None,
                reopened_by_id: None,
                reopened_by_name: None,
                reopened_reason: None,
                has_late_movements: false,
                total_cash_sales_cents: 1_150_000,
                total_card_sales_cents: 550_000,
                total_online_sales_cents: 150_000,
                total_credit_sales_cents: 0,
                total_cash_refunds_cents: 50_000,
                total_pay_in_cents: 50_000,
                total_pay_out_cents: 30_000,
                total_owner_withdrawals_cents: 100_000,
                total_cash_drops_cents: 700_000,
                created_at: "2026-10-21T08:30:00Z".parse().unwrap(),
                updated_at: "2026-10-21T18:00:00Z".parse().unwrap(),
                version: 1,
            },
            gross_sales_cents: 1_850_000,
            returns_cents: 50_000,
            net_sales_cents: 1_800_000,
            discount_cents: 25_000,
            total_invoices_count: 24,
            payment_methods: vec![
                ZReportPaymentMethodSummary {
                    method: "cash".to_string(),
                    count: 14,
                    total_cents: 1_150_000,
                },
                ZReportPaymentMethodSummary {
                    method: "card".to_string(),
                    count: 8,
                    total_cents: 550_000,
                },
            ],
            cash_movements: vec![CashMovement {
                id: "64b0000000000000000000d1".to_string(),
                key: "cm_test".to_string(),
                movement_number: "CM-000101".to_string(),
                movement_type: CashMovementType::AddCash,
                direction: CashDirection::In,
                amount_cents: 50_000,
                session_key: Some("ses_test".to_string()),
                terminal_id: "TERM-01".to_string(),
                category_key: "ccat_add_cash_test".to_string(),
                category_name: "Change float top-up".to_string(),
                custom_label: None,
                payee_key: None,
                payee_name: Some("Main Safe".to_string()),
                period_start: None,
                period_end: None,
                reference_no: None,
                note: None,
                approval_reason: None,
                reverses_key: None,
                reversed_by_key: None,
                undo_until: None,
                recorded_by_user_id: "usr_admin".to_string(),
                recorded_by_name_snapshot: "Kasun Perera".to_string(),
                approved_by_user_id: None,
                approved_by_name_snapshot: None,
                recorded_late: false,
                original_variance_session_key: None,
                created_at: "2026-10-21T10:15:00Z".parse().unwrap(),
                updated_at: "2026-10-21T10:15:00Z".parse().unwrap(),
                version: 1,
            }],
            movement_summary: vec![],
            cash_reconciliation: ZReportCashReconciliation {
                opening_float_cents: 200_000,
                cash_sales_cents: 1_150_000,
                pay_in_cents: 50_000,
                pay_out_cents: 30_000,
                cash_refunds_cents: 50_000,
                owner_withdrawals_cents: 100_000,
                cash_drops_cents: 700_000,
                expected_cash_cents: 520_000,
                counted_cash_cents: 520_000,
                variance_cents: 0,
                closing_float_cents: 200_000,
                safe_deposit_cents: 320_000,
            },
            generated_at: "2026-10-21T18:02:00Z".parse().unwrap(),
        }
    }

    #[test]
    fn z_report_payload_matches_published_schema() {
        let shop = cash_sale_invoice().shop_profile_snapshot;
        assert_matches_schema("z-report", &build_z_report_data(&base_z_report(), &shop));

        // Open session with minimal/empty shop profile: must still satisfy contract
        let mut open_report = base_z_report();
        open_report.session.status = crate::domain::sessions::SessionStatus::Open;
        open_report.session.closed_at = None;
        assert_matches_schema("z-report", &build_z_report_data(&open_report, &json!({})));
    }

    #[test]
    fn z_report_payload_carries_exact_reconciliation_math() {
        let shop = json!({ "tradingName": "TechFix", "logoBase64": "data:image/png;base64,AAAA" });
        let payload = build_z_report_data(&base_z_report(), &shop);
        assert_eq!(payload["sessionNumber"], "SES-000042");
        assert_eq!(payload["terminalId"], "TERM-01");
        assert_eq!(payload["cashierName"], "Kasun Perera");
        assert_eq!(payload["status"], "closed");
        assert_eq!(payload["grossSalesCents"], 1_850_000);
        assert_eq!(payload["returnsCents"], 50_000);
        assert_eq!(payload["netSalesCents"], 1_800_000);
        assert_eq!(payload["discountCents"], 25_000);
        assert_eq!(payload["totalInvoicesCount"], 24);
        assert_eq!(payload["cashReconciliation"]["expectedCashCents"], 520_000);
        assert_eq!(payload["cashReconciliation"]["countedCashCents"], 520_000);
        assert_eq!(payload["cashReconciliation"]["varianceCents"], 0);
    }
}
