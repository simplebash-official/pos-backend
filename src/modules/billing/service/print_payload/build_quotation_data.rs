use super::number_to_words_rupees::number_to_words_rupees;
use super::shop_address_lines::shop_address_lines;
use super::shop_field_or_empty::shop_field_or_empty;

use crate::domain::quotations::Quotation;

use serde_json::{Value, json};

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
