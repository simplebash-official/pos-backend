//! Upgrades a backup taken before a repair job or print job recorded the invoice that billed it.

use crate::modules::billing::migration::{InvoiceFacts, InvoiceLine, TicketKind, plan};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

fn text_of(row: &Value, column: &str) -> Option<String> {
    row.get(column)?.as_str().map(str::to_string)
}

fn facts_of(invoice: &Value) -> Option<InvoiceFacts> {
    if invoice.get("deleted_at").is_some_and(|day| !day.is_null()) {
        return None;
    }
    let lines: Vec<InvoiceLine> = serde_json::from_str(&text_of(invoice, "items")?).ok()?;
    let created_at = DateTime::parse_from_rfc3339(&text_of(invoice, "created_at")?)
        .ok()?
        .with_timezone(&Utc);
    Some(InvoiceFacts {
        key: text_of(invoice, "key")?,
        number: text_of(invoice, "invoice_number")?,
        created_at,
        is_voided: text_of(invoice, "status")? == "voided",
        tickets: lines.iter().filter_map(InvoiceLine::ticket).collect(),
    })
}

/// A backup taken before tickets recorded their invoice holds repair and print job rows with no
/// `billed_invoice_key`. Marks them in memory exactly the way
/// `billing::migration::migrate_sqlite` marks a live database. An invoice row that cannot be read
/// marks nothing; the ticket then simply shows no invoice, as it did before.
pub(super) fn upgrade_pre_ticket_billing_backup(tables: &mut HashMap<String, Vec<Value>>) {
    let invoices: Vec<InvoiceFacts> = tables
        .get("invoices")
        .into_iter()
        .flatten()
        .filter_map(facts_of)
        .collect();
    let mut marked = HashSet::new();
    for (table, kind) in [
        ("repairs", TicketKind::Repair),
        ("print_jobs", TicketKind::PrintJob),
    ] {
        for row in tables.get(table).into_iter().flatten() {
            if row
                .get("billed_invoice_key")
                .is_some_and(|key| !key.is_null())
                && let Some(key) = text_of(row, "key")
            {
                marked.insert((kind, key));
            }
        }
    }
    for stamp in plan(&invoices, &marked) {
        let table = match stamp.kind {
            TicketKind::Repair => "repairs",
            TicketKind::PrintJob => "print_jobs",
        };
        let row =
            tables.get_mut(table).into_iter().flatten().find(|row| {
                row.get("key").and_then(Value::as_str) == Some(stamp.ticket_key.as_str())
            });
        if let Some(row) = row.and_then(Value::as_object_mut) {
            row.insert("billed_invoice_key".into(), json!(stamp.invoice_key));
            row.insert("billed_invoice_number".into(), json!(stamp.invoice_number));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoice(key: &str, status: &str, created_at: &str, items: &Value) -> Value {
        json!({
            "key": key, "invoice_number": format!("N-{key}"), "status": status,
            "created_at": created_at, "deleted_at": null, "items": items.to_string(),
        })
    }

    #[test]
    fn a_billed_ticket_gets_its_invoice_and_a_voided_one_is_skipped() {
        let items = json!([{ "sourceType": "repair", "sourceTicketKey": "rep_1" }]);
        let mut tables = HashMap::from([
            (
                "invoices".to_string(),
                vec![
                    invoice("inv_void", "voided", "2026-01-01T09:00:00+00:00", &items),
                    invoice("inv_ok", "paid", "2026-01-05T09:00:00+00:00", &items),
                ],
            ),
            (
                "repairs".to_string(),
                vec![
                    json!({ "key": "rep_1" }),
                    json!({ "key": "rep_2", "billed_invoice_key": null }),
                ],
            ),
        ]);
        upgrade_pre_ticket_billing_backup(&mut tables);
        assert_eq!(tables["repairs"][0]["billed_invoice_key"], "inv_ok");
        assert_eq!(tables["repairs"][0]["billed_invoice_number"], "N-inv_ok");
        assert!(tables["repairs"][1].get("billed_invoice_number").is_none());
    }

    #[test]
    fn a_ticket_that_already_has_a_mark_is_left_alone() {
        let items = json!([{ "sourceType": "print", "sourceTicketKey": "prn_1" }]);
        let mut tables = HashMap::from([
            (
                "invoices".to_string(),
                vec![invoice(
                    "inv_1",
                    "paid",
                    "2026-01-05T09:00:00+00:00",
                    &items,
                )],
            ),
            (
                "print_jobs".to_string(),
                vec![json!({ "key": "prn_1", "billed_invoice_key": "inv_other" })],
            ),
        ]);
        upgrade_pre_ticket_billing_backup(&mut tables);
        assert_eq!(tables["print_jobs"][0]["billed_invoice_key"], "inv_other");
    }
}
