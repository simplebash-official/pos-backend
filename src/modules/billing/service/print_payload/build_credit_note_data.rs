use super::shop_address_lines::shop_address_lines;
use super::shop_bool::shop_bool;
use super::shop_field_or_empty::shop_field_or_empty;
use crate::domain::billing::{CreditNote, Invoice, ItemCondition, ItemDisposition};

use serde_json::{Value, json};

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
