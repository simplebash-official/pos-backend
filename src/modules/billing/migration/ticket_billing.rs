//! The pure rule that decides which invoice billed a repair job or print job saved before tickets
//! recorded it. No I/O, so SQLite, MongoDB and old backups all apply exactly the same decisions.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

/// Which kind of ticket an invoice line billed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TicketKind {
    Repair,
    PrintJob,
}

/// The two fields of a stored invoice line the rule reads, in the shape the line is stored in
/// (both SQLite's JSON column and a MongoDB document, camelCase).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    pub source_type: String,
    pub source_ticket_key: Option<String>,
}

impl InvoiceLine {
    /// The ticket this line billed, if it is a repair or print job line that names one.
    #[must_use]
    pub fn ticket(&self) -> Option<BilledTicket> {
        let kind = match self.source_type.as_str() {
            "repair" => TicketKind::Repair,
            "print" => TicketKind::PrintJob,
            _ => return None,
        };
        self.source_ticket_key.as_ref().map(|key| BilledTicket {
            kind,
            key: key.clone(),
        })
    }
}

/// One ticket line of an invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BilledTicket {
    pub kind: TicketKind,
    pub key: String,
}

/// What the rule needs to know about one invoice.
#[derive(Debug, Clone)]
pub struct InvoiceFacts {
    pub key: String,
    pub number: String,
    pub created_at: DateTime<Utc>,
    /// A voided invoice billed nothing, so it never marks a ticket.
    pub is_voided: bool,
    /// The repair and print job lines of the invoice that name their ticket.
    pub tickets: Vec<BilledTicket>,
}

/// One ticket and the invoice that billed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub kind: TicketKind,
    pub ticket_key: String,
    pub invoice_key: String,
    pub invoice_number: String,
}

/// Decides what to write. A ticket in `already_marked` is left alone. A ticket named by several
/// invoices that are not voided (a job billed again after being delivered by hand) is marked
/// with the oldest of them, ties broken by key, so the desktop and the cloud, each migrating on
/// its own, choose the same invoice. The result is ordered by ticket, so it is the same on every
/// run.
#[must_use]
pub fn plan(
    invoices: &[InvoiceFacts],
    already_marked: &HashSet<(TicketKind, String)>,
) -> Vec<Stamp> {
    let mut oldest: BTreeMap<(TicketKind, &str), &InvoiceFacts> = BTreeMap::new();
    for invoice in invoices.iter().filter(|invoice| !invoice.is_voided) {
        for ticket in &invoice.tickets {
            if already_marked.contains(&(ticket.kind, ticket.key.clone())) {
                continue;
            }
            oldest
                .entry((ticket.kind, ticket.key.as_str()))
                .and_modify(|best| {
                    if (invoice.created_at, &invoice.key) < (best.created_at, &best.key) {
                        *best = invoice;
                    }
                })
                .or_insert(invoice);
        }
    }
    oldest
        .into_iter()
        .map(|((kind, ticket_key), invoice)| Stamp {
            kind,
            ticket_key: ticket_key.to_string(),
            invoice_key: invoice.key.clone(),
            invoice_number: invoice.number.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(n: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, n, 9, 0, 0)
            .single()
            .expect("a valid date")
    }

    fn invoice(key: &str, on: u32, voided: bool, tickets: &[(TicketKind, &str)]) -> InvoiceFacts {
        InvoiceFacts {
            key: key.into(),
            number: format!("N-{key}"),
            created_at: day(on),
            is_voided: voided,
            tickets: tickets
                .iter()
                .map(|(kind, key)| BilledTicket {
                    kind: *kind,
                    key: (*key).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_ticket_is_marked_with_the_invoice_that_billed_it() {
        let invoices = [invoice(
            "inv_1",
            3,
            false,
            &[
                (TicketKind::Repair, "rep_1"),
                (TicketKind::PrintJob, "prn_1"),
            ],
        )];
        let stamps = plan(&invoices, &HashSet::new());
        assert_eq!(
            stamps,
            vec![
                Stamp {
                    kind: TicketKind::Repair,
                    ticket_key: "rep_1".into(),
                    invoice_key: "inv_1".into(),
                    invoice_number: "N-inv_1".into(),
                },
                Stamp {
                    kind: TicketKind::PrintJob,
                    ticket_key: "prn_1".into(),
                    invoice_key: "inv_1".into(),
                    invoice_number: "N-inv_1".into(),
                },
            ]
        );
    }

    #[test]
    fn a_voided_invoice_marks_nothing() {
        let invoices = [invoice("inv_1", 3, true, &[(TicketKind::Repair, "rep_1")])];
        assert!(plan(&invoices, &HashSet::new()).is_empty());
    }

    #[test]
    fn the_oldest_invoice_wins_and_a_tie_goes_to_the_smaller_key() {
        let invoices = [
            invoice("inv_b", 5, false, &[(TicketKind::Repair, "rep_1")]),
            invoice("inv_a", 5, false, &[(TicketKind::Repair, "rep_1")]),
            invoice("inv_c", 2, false, &[(TicketKind::Repair, "rep_2")]),
            invoice("inv_d", 9, false, &[(TicketKind::Repair, "rep_2")]),
        ];
        let stamps = plan(&invoices, &HashSet::new());
        assert_eq!(stamps[0].invoice_key, "inv_a");
        assert_eq!(stamps[1].invoice_key, "inv_c");
    }

    #[test]
    fn a_voided_older_invoice_does_not_beat_a_live_newer_one() {
        let invoices = [
            invoice("inv_old", 1, true, &[(TicketKind::Repair, "rep_1")]),
            invoice("inv_new", 8, false, &[(TicketKind::Repair, "rep_1")]),
        ];
        assert_eq!(plan(&invoices, &HashSet::new())[0].invoice_key, "inv_new");
    }

    #[test]
    fn a_ticket_that_is_already_marked_is_left_alone() {
        let invoices = [invoice("inv_1", 3, false, &[(TicketKind::Repair, "rep_1")])];
        let marked = HashSet::from([(TicketKind::Repair, "rep_1".to_string())]);
        assert!(plan(&invoices, &marked).is_empty());
    }

    #[test]
    fn a_repair_and_a_print_job_with_the_same_key_are_different_tickets() {
        let invoices = [invoice("inv_1", 3, false, &[(TicketKind::PrintJob, "x_1")])];
        let marked = HashSet::from([(TicketKind::Repair, "x_1".to_string())]);
        assert_eq!(plan(&invoices, &marked).len(), 1);
    }
}
