//! Upgrades a backup taken before warranty records existed.

use crate::clients::sqlite::{bson_to_iso, generate_id_hex, parse_iso_datetime};
use crate::domain::billing::InvoiceItem;
use crate::modules::warranties::migration::{
    LegacyInvoice, LegacySoldUnit, MigratedWarranty, legacy_profile, legacy_warranty,
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn text<'a>(row: &'a Value, field: &str) -> Option<&'a str> {
    row.get(field).and_then(Value::as_str)
}

fn enum_text<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("a warranty enum serializes to JSON")
}

/// Rows for `warranties` and `warranty_events` from one migrated warranty.
fn rows(migrated: &MigratedWarranty) -> (Value, Value) {
    let w = &migrated.warranty;
    let warranty = json!({
        "key": w.key, "id": generate_id_hex(), "warranty_number": w.warranty_number,
        "source_type": enum_text(&w.source_type), "invoice_key": w.invoice_key,
        "invoice_number": w.invoice_number, "line_index": w.line_index,
        "product_key": w.product_key, "repair_key": w.repair_key,
        "repair_item_key": w.repair_item_key, "item_name": w.item_name,
        "serial_number": w.serial_number, "quantity": w.quantity,
        "customer_key": w.customer_key, "customer_name": w.customer_name,
        "customer_phone": w.customer_phone, "customer_phone_tail": w.customer_phone_tail,
        "coverage": serde_json::to_string(&w.coverage).expect("a coverage serializes"),
        "warranty_type": enum_text(&w.warranty_type), "terms": w.terms,
        "unit_price_cents": w.unit_price_cents, "starts_on": w.starts_on, "ends_on": w.ends_on,
        "status": enum_text(&w.status), "void_reason": w.void_reason,
        "replaces_warranty_key": null, "replaced_by_warranty_key": null,
        "claim_count": w.claim_count, "version": w.version,
        "created_at": bson_to_iso(&w.created_at), "updated_at": bson_to_iso(&w.updated_at),
        "updated_by_device": null,
    });
    let e = &migrated.created;
    let event = json!({
        "key": e.key, "id": generate_id_hex(), "warranty_key": e.warranty_key,
        "claim_key": null, "kind": enum_text(&e.kind), "from_status": null,
        "to_status": e.to_status, "note": e.note, "actor_user_id": e.actor_user_id,
        "actor_name": e.actor_name, "approved_by_user_id": null, "approved_by_name": null,
        "version": e.version, "created_at": bson_to_iso(&e.created_at),
        "updated_at": bson_to_iso(&e.updated_at), "updated_by_device": null,
    });
    (warranty, event)
}

/// A backup taken before warranty records carries `warranty_months` on its products and
/// `warranty_months`/`warranty_expires_at` on sold serials, and no `warranties` table. Rewrites
/// it, in memory, exactly the way `warranties::migration::migrate_sqlite` rewrites a live
/// database: each product gets the profile its month count describes, and each sold serial
/// that carried a warranty gets its record under the same deterministic key. A backup that
/// already has the `warranties` table is left untouched.
pub(super) fn upgrade_pre_warranty_backup(tables: &mut HashMap<String, Vec<Value>>) {
    if tables.contains_key("warranties") {
        return;
    }
    for product in tables.get_mut("products").into_iter().flatten() {
        let Some(row) = product.as_object_mut() else {
            continue;
        };
        let months = row.remove("warranty_months").and_then(|v| v.as_i64());
        row.insert(
            "warranty".into(),
            json!(
                serde_json::to_string(&legacy_profile(months))
                    .expect("a product warranty serializes")
            ),
        );
    }

    let invoices: HashMap<String, Value> = tables
        .get("invoices")
        .into_iter()
        .flatten()
        .filter_map(|row| Some((text(row, "key")?.to_string(), row.clone())))
        .collect();
    let mut warranties = Vec::new();
    let mut events = Vec::new();
    for serial in tables.get_mut("product_serials").into_iter().flatten() {
        let months = serial.get("warranty_months").and_then(Value::as_i64);
        if let Some(row) = serial.as_object_mut() {
            row.remove("warranty_months");
            row.remove("warranty_expires_at");
        }
        let (Some(months), Some(invoice_key), Some(sold_at)) = (
            months.filter(|m| *m > 0),
            text(serial, "invoice_key"),
            text(serial, "sold_at"),
        ) else {
            continue;
        };
        let Some(invoice) = invoices.get(invoice_key) else {
            continue;
        };
        let Some(items) = text(invoice, "items")
            .and_then(|json| serde_json::from_str::<Vec<InvoiceItem>>(json).ok())
        else {
            continue;
        };
        let (Ok(sold_at), Some(serial_key), Some(serial_number), Some(product_key)) = (
            parse_iso_datetime(sold_at),
            text(serial, "key"),
            text(serial, "serial_number"),
            text(serial, "product_key"),
        ) else {
            continue;
        };
        let unit = LegacySoldUnit {
            serial_key,
            serial_number,
            product_key,
            serial_status: text(serial, "status").unwrap_or("sold"),
            months,
            sold_at,
            invoice: LegacyInvoice {
                key: invoice_key,
                number: text(invoice, "invoice_number").unwrap_or(invoice_key),
                status: text(invoice, "status").unwrap_or("paid"),
                customer_key: text(invoice, "customer_key"),
                customer_name: text(invoice, "customer_name_snapshot"),
                customer_phone: text(invoice, "customer_phone_snapshot"),
                items: &items,
            },
        };
        if let Ok(migrated) = legacy_warranty(&unit) {
            let (warranty, event) = rows(&migrated);
            warranties.push(warranty);
            events.push(event);
        }
    }
    tables.insert("warranties".to_string(), warranties);
    tables.insert("warranty_events".to_string(), events);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_backup_gets_profiles_and_warranty_records() {
        let items = json!([{
            "productKey": "prd_1", "name": "Phone X", "sku": null, "unitPriceCents": 90_000,
            "quantity": 1, "discountCents": 0, "totalCents": 90_000, "unitCostCents": null,
            "sourceType": "retail", "sourceTicketKey": null, "sourceTicketNumber": null,
            "assignedEmployeeName": null, "returnedQuantity": 0,
            "serialNumbers": ["IMEI-1"], "batchAllocations": []
        }]);
        let mut tables: HashMap<String, Vec<Value>> = HashMap::from([
            (
                "products".to_string(),
                vec![
                    json!({ "key": "prd_1", "warranty_months": 6 }),
                    json!({ "key": "prd_2", "warranty_months": null }),
                ],
            ),
            (
                "product_serials".to_string(),
                vec![json!({
                    "key": "psn_1", "serial_number": "IMEI-1", "product_key": "prd_1",
                    "status": "sold", "invoice_key": "inv_1",
                    "sold_at": "2026-10-01T04:00:00Z", "warranty_months": 6,
                    "warranty_expires_at": "2027-03-30T04:00:00Z"
                })],
            ),
            (
                "invoices".to_string(),
                vec![json!({
                    "key": "inv_1", "invoice_number": "INV-000001", "status": "paid",
                    "items": items.to_string()
                })],
            ),
        ]);
        upgrade_pre_warranty_backup(&mut tables);

        let profile: Value = serde_json::from_str(
            tables["products"][0]["warranty"]
                .as_str()
                .expect("the profile is JSON text"),
        )
        .expect("the profile parses");
        assert_eq!(profile["coverage"]["value"], 6);
        assert_eq!(profile["warrantyType"], "shop");
        assert!(tables["products"][0].get("warranty_months").is_none());
        assert!(
            tables["product_serials"][0]
                .get("warranty_expires_at")
                .is_none()
        );

        let warranty = &tables["warranties"][0];
        assert_eq!(warranty["key"], "wty_mig_psn_1");
        assert_eq!(warranty["ends_on"], "2027-03-31");
        assert_eq!(warranty["status"], "active");
        assert_eq!(tables["warranty_events"][0]["key"], "wev_mig_psn_1");

        let again = tables.clone();
        upgrade_pre_warranty_backup(&mut tables);
        assert_eq!(tables, again, "a backup that has warranties is left alone");
    }
}
