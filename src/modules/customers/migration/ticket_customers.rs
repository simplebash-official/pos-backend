//! The pure rule that links tickets saved before a customer was required to a customer record.
//! No I/O, so SQLite, MongoDB and old backups all apply exactly the same decisions.

use crate::core::constants::prefixes;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// How many trailing digits of a phone number identify a person (country code and leading zero
/// do not count), the same rule the warranty lookup uses.
const PHONE_IDENTITY_DIGITS: usize = 9;

/// Which kind of ticket a link belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketKind {
    Repair,
    PrintJob,
}

/// A ticket that was saved with only a typed name and phone.
#[derive(Debug, Clone)]
pub struct UnlinkedTicket {
    pub kind: TicketKind,
    pub key: String,
    pub name: String,
    pub phone: String,
    pub created_at: DateTime<Utc>,
}

/// A customer that is not deleted and so can be linked to.
#[derive(Debug, Clone)]
pub struct KnownCustomer {
    pub key: String,
    pub phone: String,
    pub created_at: DateTime<Utc>,
}

/// A customer the migration has to create because nobody has that phone number yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCustomer {
    pub key: String,
    pub name: String,
    pub phone: String,
    pub created_at: DateTime<Utc>,
}

/// One ticket and the customer it is now linked to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: TicketKind,
    pub ticket_key: String,
    pub customer_key: String,
}

/// What a run has to write.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub new_customers: Vec<NewCustomer>,
    pub links: Vec<Link>,
}

/// The part of a phone number that identifies its owner: the last nine digits, or the typed
/// text itself when it holds no digit at all (a number nobody could have dialled still has to
/// group its own tickets).
#[must_use]
pub fn phone_identity(phone: &str) -> String {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return phone.trim().to_string();
    }
    let start = digits.len().saturating_sub(PHONE_IDENTITY_DIGITS);
    digits[start..].to_string()
}

/// The key of a customer made by this migration, derived from the phone identity so the
/// desktop and the cloud, each migrating on its own, create the same row.
#[must_use]
pub fn migrated_customer_key(identity: &str) -> String {
    let digest = Sha256::digest(identity.as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{}_mig_{hex}", prefixes::CUSTOMER)
}

/// Decides, for every unlinked ticket, the customer it joins: the oldest existing customer with
/// the same phone identity, otherwise a customer made from the oldest ticket with that phone.
#[must_use]
pub fn plan(tickets: &[UnlinkedTicket], customers: &[KnownCustomer]) -> Plan {
    let mut oldest_customer: HashMap<String, &KnownCustomer> = HashMap::new();
    for customer in customers {
        oldest_customer
            .entry(phone_identity(&customer.phone))
            .and_modify(|best| {
                if (customer.created_at, &customer.key) < (best.created_at, &best.key) {
                    *best = customer;
                }
            })
            .or_insert(customer);
    }

    let mut groups: BTreeMap<String, Vec<&UnlinkedTicket>> = BTreeMap::new();
    for ticket in tickets {
        groups
            .entry(phone_identity(&ticket.phone))
            .or_default()
            .push(ticket);
    }

    let mut plan = Plan::default();
    for (identity, mut group) in groups {
        group.sort_by(|a, b| (a.created_at, &a.key).cmp(&(b.created_at, &b.key)));
        let customer_key = match oldest_customer.get(&identity) {
            Some(customer) => customer.key.clone(),
            None => {
                let first = group[0];
                let key = migrated_customer_key(&identity);
                plan.new_customers.push(NewCustomer {
                    key: key.clone(),
                    name: first.name.trim().to_string(),
                    phone: first.phone.trim().to_string(),
                    created_at: first.created_at,
                });
                key
            }
        };
        for ticket in group {
            plan.links.push(Link {
                kind: ticket.kind,
                ticket_key: ticket.key.clone(),
                customer_key: customer_key.clone(),
            });
        }
    }
    plan
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

    fn ticket(kind: TicketKind, key: &str, name: &str, phone: &str, on: u32) -> UnlinkedTicket {
        UnlinkedTicket {
            kind,
            key: key.into(),
            name: name.into(),
            phone: phone.into(),
            created_at: day(on),
        }
    }

    fn customer(key: &str, phone: &str, on: u32) -> KnownCustomer {
        KnownCustomer {
            key: key.into(),
            phone: phone.into(),
            created_at: day(on),
        }
    }

    #[test]
    fn a_phone_is_identified_by_its_last_nine_digits() {
        assert_eq!(phone_identity("+94 77-123 4567"), "771234567");
        assert_eq!(phone_identity("0771234567"), "771234567");
        assert_eq!(phone_identity("12345"), "12345");
        assert_eq!(phone_identity(" --- "), "---");
    }

    #[test]
    fn a_ticket_joins_the_oldest_customer_with_the_same_phone() {
        let customers = [
            customer("cus_new", "0771234567", 5),
            customer("cus_old", "+94771234567", 2),
        ];
        let tickets = [ticket(
            TicketKind::Repair,
            "rep_1",
            "Nimal",
            "077 123 4567",
            9,
        )];
        let plan = plan(&tickets, &customers);
        assert!(plan.new_customers.is_empty());
        assert_eq!(plan.links.len(), 1);
        assert_eq!(plan.links[0].customer_key, "cus_old");
    }

    #[test]
    fn tickets_of_one_phone_share_one_new_customer_named_from_the_oldest_ticket() {
        let tickets = [
            ticket(TicketKind::PrintJob, "prn_2", "Kamal P.", "0712223334", 8),
            ticket(
                TicketKind::Repair,
                "rep_1",
                "Kamal Perera",
                "071 222 3334",
                3,
            ),
        ];
        let plan = plan(&tickets, &[]);
        assert_eq!(plan.new_customers.len(), 1);
        let made = &plan.new_customers[0];
        assert_eq!(made.name, "Kamal Perera");
        assert_eq!(made.phone, "071 222 3334");
        assert_eq!(made.created_at, day(3));
        assert_eq!(made.key, migrated_customer_key("712223334"));
        assert!(plan.links.iter().all(|l| l.customer_key == made.key));
        assert_eq!(plan.links.len(), 2);
    }

    #[test]
    fn the_new_customer_key_is_the_same_on_every_run() {
        assert_eq!(
            migrated_customer_key("712223334"),
            migrated_customer_key("712223334")
        );
        assert_ne!(
            migrated_customer_key("712223334"),
            migrated_customer_key("712223335")
        );
        assert!(migrated_customer_key("1").starts_with(&format!("{}_mig_", prefixes::CUSTOMER)));
    }

    #[test]
    fn different_phones_get_different_customers() {
        let tickets = [
            ticket(TicketKind::Repair, "rep_1", "A", "0711111111", 1),
            ticket(TicketKind::Repair, "rep_2", "B", "0722222222", 2),
        ];
        let plan = plan(&tickets, &[]);
        assert_eq!(plan.new_customers.len(), 2);
        assert_ne!(plan.links[0].customer_key, plan.links[1].customer_key);
    }
}
