//! Applies the ticket-to-customer links decided by `plan` to a live SQLite database or to a
//! MongoDB database holding any number of shops.

use super::ticket_customers::{KnownCustomer, NewCustomer, TicketKind, UnlinkedTicket, plan};
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::TENANT_FIELD;
use crate::modules::customers::model::CustomerDocument;
use crate::modules::customers::repository;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, DateTime as BsonDateTime, Document, doc};
use mongodb::options::UpdateOptions;
use sqlx::{Executor, Row, SqlitePool};
use std::collections::BTreeMap;

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Tickets that had no customer.
    pub tickets: u64,
    /// Customers created because nobody had the ticket's phone number.
    pub customers_created: u64,
}

fn protocol(err: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(err.to_string())
}

fn table_of(kind: TicketKind) -> &'static str {
    match kind {
        TicketKind::Repair => "repairs",
        TicketKind::PrintJob => "print_jobs",
    }
}

fn new_customer_document(customer: &NewCustomer) -> CustomerDocument {
    let created_at = BsonDateTime::from_chrono(customer.created_at);
    CustomerDocument {
        id: None,
        key: customer.key.clone(),
        name: customer.name.clone(),
        contact_person: None,
        primary_phone: customer.phone.clone(),
        secondary_phone: None,
        email: None,
        address: None,
        tags: Vec::new(),
        notes: None,
        outstanding_balance_cents: 0,
        total_purchases_cents: 0,
        version: 1,
        created_at,
        updated_at: created_at,
        deleted_at: None,
        updated_by_device: None,
    }
}

fn parse_day(text: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    DateTime::parse_from_rfc3339(text)
        .map(|day| day.with_timezone(&Utc))
        .map_err(protocol)
}

/// Gives an old SQLite database's unlinked repair jobs and print jobs a customer, once. Does
/// nothing when every ticket already has one. The sync triggers are dropped first
/// (`install_triggers` puts them back) so none of this is uploaded: the cloud migrates itself
/// with the same rule and the same new-customer keys.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tickets = Vec::new();
    for kind in [TicketKind::Repair, TicketKind::PrintJob] {
        let rows = sqlx::query(&format!(
            "SELECT key, customer_name, customer_phone, created_at FROM {} \
             WHERE customer_key IS NULL",
            table_of(kind)
        ))
        .fetch_all(pool)
        .await?;
        for row in rows {
            tickets.push(UnlinkedTicket {
                kind,
                key: row.get("key"),
                name: row.get("customer_name"),
                phone: row.get("customer_phone"),
                created_at: parse_day(&row.get::<String, _>("created_at"))?,
            });
        }
    }
    if tickets.is_empty() {
        return Ok(());
    }
    tracing::info!(
        "Linking {} repair and print jobs to customers",
        tickets.len()
    );
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await?;
    for trigger in triggers {
        pool.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }

    let mut customers = Vec::new();
    for row in
        sqlx::query("SELECT key, primary_phone, created_at FROM customers WHERE deleted_at IS NULL")
            .fetch_all(pool)
            .await?
    {
        customers.push(KnownCustomer {
            key: row.get("key"),
            phone: row.get("primary_phone"),
            created_at: parse_day(&row.get::<String, _>("created_at"))?,
        });
    }

    let plan = plan(&tickets, &customers);
    let db = Db::Sqlite(pool.clone());
    for customer in &plan.new_customers {
        repository::insert_customer(&db, new_customer_document(customer))
            .await
            .map_err(protocol)?;
    }
    for link in &plan.links {
        sqlx::query(&format!(
            "UPDATE {} SET customer_key = ? WHERE key = ?",
            table_of(link.kind)
        ))
        .bind(&link.customer_key)
        .bind(&link.ticket_key)
        .execute(pool)
        .await?;
    }
    Ok(())
}

fn text<'a>(document: &'a Document, collection: &str, field: &str) -> AppResult<&'a str> {
    document
        .get_str(field)
        .map_err(|e| AppError::internal(format!("{collection}.{field}: {e}")))
}

/// Narrows a filter to one shop, or to the documents that carry no shop at all (a single-shop
/// database).
fn in_shop(shop: &Option<String>, mut filter: Document) -> Document {
    match shop {
        Some(shop) => filter.insert(TENANT_FIELD, shop.as_str()),
        None => filter.insert(TENANT_FIELD, Bson::Null),
    };
    filter
}

/// Gives every shop's unlinked repair jobs and print jobs on a MongoDB database a customer
/// (soft-deleted tickets included). Idempotent: a ticket that has a customer is left alone, and
/// a customer made by an earlier run is found by its phone number.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let unlinked = doc! { "$or": [
        { "customer_key": { "$exists": false } },
        { "customer_key": Bson::Null },
    ] };
    let mut shops: BTreeMap<Option<String>, Vec<UnlinkedTicket>> = BTreeMap::new();
    for kind in [TicketKind::Repair, TicketKind::PrintJob] {
        let name = table_of(kind);
        let mut cursor = db
            .collection::<Document>(name)
            .find(unlinked.clone())
            .await?;
        while let Some(ticket) = cursor.try_next().await? {
            let shop = ticket.get_str(TENANT_FIELD).ok().map(str::to_string);
            shops.entry(shop).or_default().push(UnlinkedTicket {
                kind,
                key: text(&ticket, name, "key")?.to_string(),
                name: text(&ticket, name, "customer_name")?.to_string(),
                phone: text(&ticket, name, "customer_phone")?.to_string(),
                created_at: ticket
                    .get_datetime("created_at")
                    .map_err(|e| AppError::internal(format!("{name}.created_at: {e}")))?
                    .to_chrono(),
            });
        }
    }

    let customers = db.collection::<Document>("customers");
    let mut report = Report::default();
    for (shop, tickets) in shops {
        let mut known = Vec::new();
        let mut cursor = customers
            .find(in_shop(&shop, doc! { "deleted_at": Bson::Null }))
            .await?;
        while let Some(customer) = cursor.try_next().await? {
            known.push(KnownCustomer {
                key: text(&customer, "customers", "key")?.to_string(),
                phone: text(&customer, "customers", "primary_phone")?.to_string(),
                created_at: customer
                    .get_datetime("created_at")
                    .map_err(|e| AppError::internal(format!("customers.created_at: {e}")))?
                    .to_chrono(),
            });
        }

        let plan = plan(&tickets, &known);
        report.tickets += plan.links.len() as u64;
        report.customers_created += plan.new_customers.len() as u64;
        if mode != Mode::Apply {
            continue;
        }
        for customer in &plan.new_customers {
            let mut fresh = mongodb::bson::serialize_to_document(&new_customer_document(customer))?;
            if let Some(shop) = &shop {
                fresh.insert(TENANT_FIELD, shop.as_str());
            }
            customers
                .update_one(
                    in_shop(&shop, doc! { "key": &customer.key }),
                    doc! { "$setOnInsert": fresh },
                )
                .with_options(UpdateOptions::builder().upsert(true).build())
                .await?;
        }
        for link in &plan.links {
            db.collection::<Document>(table_of(link.kind))
                .update_one(
                    in_shop(&shop, doc! { "key": &link.ticket_key }),
                    doc! { "$set": { "customer_key": &link.customer_key } },
                )
                .await?;
        }
    }
    Ok(report)
}
