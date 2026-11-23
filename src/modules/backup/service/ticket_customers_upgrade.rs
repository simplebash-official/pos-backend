//! Upgrades a backup taken before every repair job and print job had to name a customer.

use crate::modules::customers::migration::{KnownCustomer, TicketKind, UnlinkedTicket, plan};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

fn day_of(row: &Value) -> Option<DateTime<Utc>> {
    let text = row.get("created_at")?.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|day| day.with_timezone(&Utc))
}

fn text_of(row: &Value, column: &str) -> Option<String> {
    row.get(column)?.as_str().map(str::to_string)
}

/// A 24-character hexadecimal row id derived from the customer key, so the restored row is the
/// same on every restore.
fn row_id(key: &str) -> String {
    Sha256::digest(key.as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A backup taken before tickets needed a customer holds repair and print job rows whose
/// `customer_key` is null or missing. Links them in memory exactly the way
/// `customers::migration::migrate_sqlite` links a live database, adding the customers it has
/// to create. A row whose name, phone or date cannot be read keeps no customer and fails the
/// restore on `customer_key` rather than inventing one.
pub(super) fn upgrade_pre_ticket_customers_backup(tables: &mut HashMap<String, Vec<Value>>) {
    let mut tickets = Vec::new();
    for (table, kind) in [
        ("repairs", TicketKind::Repair),
        ("print_jobs", TicketKind::PrintJob),
    ] {
        for row in tables.get(table).into_iter().flatten() {
            if row.get("customer_key").is_some_and(|key| !key.is_null()) {
                continue;
            }
            let (Some(key), Some(name), Some(phone), Some(created_at)) = (
                text_of(row, "key"),
                text_of(row, "customer_name"),
                text_of(row, "customer_phone"),
                day_of(row),
            ) else {
                continue;
            };
            tickets.push(UnlinkedTicket {
                kind,
                key,
                name,
                phone,
                created_at,
            });
        }
    }
    if tickets.is_empty() {
        return;
    }

    let customers: Vec<KnownCustomer> = tables
        .get("customers")
        .into_iter()
        .flatten()
        .filter(|row| row.get("deleted_at").is_none_or(Value::is_null))
        .filter_map(|row| {
            Some(KnownCustomer {
                key: text_of(row, "key")?,
                phone: text_of(row, "primary_phone")?,
                created_at: day_of(row)?,
            })
        })
        .collect();

    let plan = plan(&tickets, &customers);
    let made = tables.entry("customers".to_string()).or_default();
    for customer in &plan.new_customers {
        let created_at = customer.created_at.to_rfc3339();
        // The other customer columns take their column defaults on restore.
        made.push(json!({
            "key": customer.key,
            "id": row_id(&customer.key),
            "name": customer.name,
            "primary_phone": customer.phone,
            "created_at": created_at,
            "updated_at": created_at,
        }));
    }
    for link in &plan.links {
        let table = match link.kind {
            TicketKind::Repair => "repairs",
            TicketKind::PrintJob => "print_jobs",
        };
        let row =
            tables.get_mut(table).into_iter().flatten().find(|row| {
                row.get("key").and_then(Value::as_str) == Some(link.ticket_key.as_str())
            });
        if let Some(row) = row.and_then(Value::as_object_mut) {
            row.insert("customer_key".into(), json!(link.customer_key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_without_a_customer_gets_one_and_the_customer_row_is_added() {
        let mut tables = HashMap::from([
            (
                "repairs".to_string(),
                vec![json!({
                    "key": "rep_old", "customer_key": null, "customer_name": "Nimal Perera",
                    "customer_phone": "0771234567", "created_at": "2026-01-02T09:00:00+00:00",
                })],
            ),
            (
                "print_jobs".to_string(),
                vec![json!({
                    "key": "prn_old", "customer_name": "Nimal P", "customer_phone": "077 123 4567",
                    "created_at": "2026-01-05T09:00:00+00:00",
                })],
            ),
        ]);
        upgrade_pre_ticket_customers_backup(&mut tables);
        let repair_customer = &tables["repairs"][0]["customer_key"];
        assert_eq!(&tables["print_jobs"][0]["customer_key"], repair_customer);
        let customers = &tables["customers"];
        assert_eq!(customers.len(), 1);
        assert_eq!(&customers[0]["key"], repair_customer);
        assert_eq!(customers[0]["name"], "Nimal Perera");
        assert_eq!(customers[0]["id"].as_str().expect("text").len(), 24);
    }

    #[test]
    fn a_ticket_that_has_a_customer_is_left_alone_and_an_existing_customer_is_reused() {
        let mut tables = HashMap::from([
            (
                "repairs".to_string(),
                vec![
                    json!({ "key": "rep_a", "customer_key": "cus_a", "customer_name": "A",
                        "customer_phone": "0700000000", "created_at": "2026-01-02T09:00:00+00:00" }),
                    json!({ "key": "rep_b", "customer_key": null, "customer_name": "B",
                        "customer_phone": "0711111111", "created_at": "2026-01-03T09:00:00+00:00" }),
                ],
            ),
            (
                "customers".to_string(),
                vec![json!({ "key": "cus_b", "primary_phone": "+94711111111",
                    "created_at": "2025-12-01T09:00:00+00:00" })],
            ),
        ]);
        upgrade_pre_ticket_customers_backup(&mut tables);
        assert_eq!(tables["repairs"][0]["customer_key"], "cus_a");
        assert_eq!(tables["repairs"][1]["customer_key"], "cus_b");
        assert_eq!(tables["customers"].len(), 1);
    }
}
