//! Pure conversions from the old warranty shape (`warranty_months` on products, and
//! `warranty_months`/`warranty_expires_at` stamped on sold serials) to the warranty records.
//! Shared by the SQLite startup migration, the MongoDB tool and the old-backup upgrader, so all
//! three produce identical rows (deterministic keys) and sync converges.

use crate::core::utils::shop_day;
use crate::domain::billing::InvoiceItem;
use crate::domain::warranties::{
    ProductWarranty, WarrantyCoverage, WarrantyEventKind, WarrantySourceType, WarrantyStatus,
    WarrantyType, WarrantyUnit,
};
use crate::modules::warranties::model::{WarrantyDocument, WarrantyEventDocument};
use crate::modules::warranties::service::phone_tail;
use chrono::{DateTime, Utc};
use mongodb::bson::DateTime as BsonDateTime;
use pos_calc_core::warranty::{self, WarrantyEnd};

/// The warranty an old product carried. The old shape had only a month count; whoever sold it
/// gave that warranty, so it becomes a shop warranty. No count, or zero, is no warranty.
#[must_use]
pub fn legacy_profile(months: Option<i64>) -> ProductWarranty {
    match months
        .and_then(|m| u32::try_from(m).ok())
        .filter(|m| *m > 0)
    {
        None => ProductWarranty::none(),
        Some(value) => ProductWarranty {
            coverage: WarrantyCoverage::Limited {
                value,
                unit: WarrantyUnit::Months,
            },
            warranty_type: Some(WarrantyType::Shop),
            terms: None,
        },
    }
}

/// The invoice an old sold serial points at.
#[derive(Debug)]
pub struct LegacyInvoice<'a> {
    pub key: &'a str,
    pub number: &'a str,
    /// The invoice's stored status text (`voided` cancels the warranty).
    pub status: &'a str,
    pub customer_key: Option<&'a str>,
    pub customer_name: Option<&'a str>,
    pub customer_phone: Option<&'a str>,
    pub items: &'a [InvoiceItem],
}

/// One old sold serial that carried a warranty.
#[derive(Debug)]
pub struct LegacySoldUnit<'a> {
    pub serial_key: &'a str,
    pub serial_number: &'a str,
    pub product_key: &'a str,
    /// The serial's stored status text; anything but `sold` means it came back.
    pub serial_status: &'a str,
    pub months: i64,
    pub sold_at: DateTime<Utc>,
    pub invoice: LegacyInvoice<'a>,
}

/// The warranty record and its first log line for one old sold serial.
#[derive(Debug)]
pub struct MigratedWarranty {
    pub warranty: WarrantyDocument,
    pub created: WarrantyEventDocument,
}

/// The deterministic key of the warranty migrated from a serial.
#[must_use]
pub fn migrated_key(serial_key: &str) -> String {
    format!("wty_mig_{serial_key}")
}

/// Builds the warranty an old sold serial had. The period is worked out again from the sale
/// day with calendar months (the old rule counted 30-day months), so it may end a day or two
/// later than the old expiry. A voided sale or a returned unit gets a void warranty.
///
/// # Errors
///
/// Why it cannot be built: the serial is not on its invoice, or the month count is out of range.
pub fn legacy_warranty(unit: &LegacySoldUnit<'_>) -> Result<MigratedWarranty, String> {
    let profile = legacy_profile(Some(unit.months));
    let Some(warranty_type) = profile.warranty_type else {
        return Err(format!(
            "serial {} has a warranty of {} months",
            unit.serial_number, unit.months
        ));
    };
    let (line_index, line) = unit
        .invoice
        .items
        .iter()
        .enumerate()
        .find(|(_, item)| item.serial_numbers.iter().any(|s| s == unit.serial_number))
        .ok_or_else(|| {
            format!(
                "serial {} is not on invoice {}",
                unit.serial_number, unit.invoice.number
            )
        })?;
    let starts_on = shop_day(unit.sold_at);
    let ends_on = match warranty::ends_on(starts_on, profile.coverage).map_err(|e| e.to_string())? {
        WarrantyEnd::On(day) => Some(day.to_string()),
        WarrantyEnd::Never => None,
    };
    let void_reason = if unit.invoice.status == "voided" {
        Some(format!("Invoice {} was voided", unit.invoice.number))
    } else if unit.serial_status != "sold" {
        Some("Returned before warranties were recorded separately".to_string())
    } else {
        None
    };
    let status = if void_reason.is_some() {
        WarrantyStatus::Void
    } else {
        WarrantyStatus::Active
    };
    let at = BsonDateTime::from_chrono(unit.sold_at);
    let key = migrated_key(unit.serial_key);
    let warranty = WarrantyDocument {
        id: None,
        key: key.clone(),
        warranty_number: format!("WTY-M-{}", unit.serial_number),
        source_type: WarrantySourceType::ProductSale,
        invoice_key: unit.invoice.key.to_string(),
        invoice_number: unit.invoice.number.to_string(),
        line_index: i64::try_from(line_index).expect("an invoice has few lines"),
        product_key: Some(unit.product_key.to_string()),
        repair_key: None,
        repair_item_key: None,
        item_name: line.name.clone(),
        serial_number: Some(unit.serial_number.to_string()),
        quantity: 1,
        customer_key: unit.invoice.customer_key.map(str::to_string),
        customer_name: unit.invoice.customer_name.map(str::to_string),
        customer_phone: unit.invoice.customer_phone.map(str::to_string),
        customer_phone_tail: unit.invoice.customer_phone.and_then(phone_tail),
        coverage: profile.coverage,
        warranty_type,
        terms: None,
        unit_price_cents: line.unit_price_cents,
        starts_on: starts_on.to_string(),
        ends_on,
        status,
        void_reason,
        replaces_warranty_key: None,
        replaced_by_warranty_key: None,
        claim_count: 0,
        version: 1,
        created_at: at,
        updated_at: at,
        updated_by_device: None,
    };
    let created = WarrantyEventDocument {
        id: None,
        key: format!("wev_mig_{}", unit.serial_key),
        warranty_key: key,
        claim_key: None,
        kind: WarrantyEventKind::Created,
        from_status: None,
        to_status: Some(status.as_str().to_string()),
        note: Some(format!(
            "Sold on {}; carried over from the old warranty records",
            unit.invoice.number
        )),
        actor_user_id: "system".to_string(),
        actor_name: "System".to_string(),
        approved_by_user_id: None,
        approved_by_name: None,
        version: 1,
        created_at: at,
        updated_at: at,
        updated_by_device: None,
    };
    Ok(MigratedWarranty { warranty, created })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(serials: &[&str]) -> InvoiceItem {
        serde_json::from_value(serde_json::json!({
            "productKey": "prd_1", "name": "Phone X", "sku": null,
            "unitPriceCents": 120_000, "quantity": serials.len(), "discountCents": 0,
            "totalCents": 120_000, "unitCostCents": null, "sourceType": "retail",
            "sourceTicketKey": null, "sourceTicketNumber": null,
            "assignedEmployeeName": null, "returnedQuantity": 0,
            "serialNumbers": serials, "batchAllocations": []
        }))
        .expect("a test line deserializes")
    }

    fn unit<'a>(
        items: &'a [InvoiceItem],
        status: &'a str,
        invoice_status: &'a str,
    ) -> LegacySoldUnit<'a> {
        LegacySoldUnit {
            serial_key: "psn_1",
            serial_number: "IMEI-2",
            product_key: "prd_1",
            serial_status: status,
            months: 12,
            // 2026-11-02 20:00 UTC is 3 November in the shop.
            sold_at: Utc
                .with_ymd_and_hms(2026, 11, 2, 20, 0, 0)
                .single()
                .expect("valid"),
            invoice: LegacyInvoice {
                key: "inv_1",
                number: "INV-000001",
                status: invoice_status,
                customer_key: None,
                customer_name: Some("Nimal"),
                customer_phone: Some("077 123 4567"),
                items,
            },
        }
    }

    #[test]
    fn old_month_counts_become_shop_warranties() {
        assert_eq!(legacy_profile(None), ProductWarranty::none());
        assert_eq!(legacy_profile(Some(0)), ProductWarranty::none());
        assert_eq!(legacy_profile(Some(-3)), ProductWarranty::none());
        let six = legacy_profile(Some(6));
        assert_eq!(
            six.coverage,
            WarrantyCoverage::Limited {
                value: 6,
                unit: WarrantyUnit::Months
            }
        );
        assert_eq!(six.warranty_type, Some(WarrantyType::Shop));
    }

    #[test]
    fn a_sold_serial_becomes_a_warranty_on_its_line() {
        let items = [item(&["IMEI-9"]), item(&["IMEI-1", "IMEI-2"])];
        let migrated = legacy_warranty(&unit(&items, "sold", "paid")).expect("migrates");
        let w = migrated.warranty;
        assert_eq!(w.key, "wty_mig_psn_1");
        assert_eq!(w.warranty_number, "WTY-M-IMEI-2");
        assert_eq!(w.line_index, 1);
        assert_eq!(w.starts_on, "2026-11-03", "the shop-local sale day");
        assert_eq!(w.ends_on.as_deref(), Some("2027-11-02"));
        assert_eq!(w.status, WarrantyStatus::Active);
        assert_eq!(w.customer_phone_tail.as_deref(), Some("771234567"));
        assert_eq!(migrated.created.key, "wev_mig_psn_1");
    }

    #[test]
    fn a_returned_unit_or_voided_sale_migrates_void() {
        let items = [item(&["IMEI-2"])];
        let returned = legacy_warranty(&unit(&items, "returned_resalable", "paid")).expect("ok");
        assert_eq!(returned.warranty.status, WarrantyStatus::Void);
        let voided = legacy_warranty(&unit(&items, "sold", "voided")).expect("ok");
        assert_eq!(voided.warranty.status, WarrantyStatus::Void);
        assert_eq!(
            voided.warranty.void_reason.as_deref(),
            Some("Invoice INV-000001 was voided")
        );
    }

    #[test]
    fn a_serial_missing_from_its_invoice_is_reported() {
        let items = [item(&["OTHER"])];
        assert!(legacy_warranty(&unit(&items, "sold", "paid")).is_err());
    }
}
