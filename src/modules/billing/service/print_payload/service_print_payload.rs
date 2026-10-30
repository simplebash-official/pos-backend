use crate::domain::billing::{CreditNote, Invoice, ItemCondition, ItemDisposition};
use crate::domain::quotations::Quotation;
use crate::domain::sessions::ZReportResponse;
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

pub(super) fn formatted_date_time(invoice: &Invoice) -> (String, String) {
    let formatted_date = invoice.created_at.format("%d %b %Y").to_string();
    let formatted_time = invoice.created_at.format("%H:%M").to_string();
    (formatted_date, formatted_time)
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

/// How much has been paid against this invoice, and how much is still owed,
/// for the printed "Amount Paid" / "Balance Due" lines. `paid_so_far_cents`
/// is the sum of every payment row (checkout deposit + later installments);
/// callers that don't have it can pass `invoice.amount_received_cents`.
pub(super) fn paid_and_balance(invoice: &Invoice, paid_so_far_cents: i64) -> (i64, i64) {
    crate::core::calculations::invoice_paid_and_balance(
        invoice.is_credit,
        invoice.total_cents,
        paid_so_far_cents,
        invoice.refunded_cents,
    )
}

pub(super) fn shop_address_lines(shop: &Value) -> Vec<&str> {
    shop.get("addressLines")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

pub(super) fn shop_bool(shop: &Value, field: &str) -> bool {
    shop.get(field).and_then(Value::as_bool).unwrap_or(false)
}

/// `invoice.shop_profile_snapshot` is the frontend's `ShopProfile` object,
/// embedded verbatim at sale-completion time (D2) — this just reads fields
/// off it by name by way of `serde_json::Value` indexing, since the
/// snapshot's shape is owned by the frontend, not modeled as a Rust struct
/// here (no backend shop-profile module exists, deliberately — see D2).
pub(super) fn shop_field<'a>(shop: &'a Value, field: &str) -> Option<&'a str> {
    shop.get(field).and_then(Value::as_str)
}

pub(super) fn shop_field_or_empty<'a>(shop: &'a Value, field: &str) -> &'a str {
    shop_field(shop, field).unwrap_or("")
}
