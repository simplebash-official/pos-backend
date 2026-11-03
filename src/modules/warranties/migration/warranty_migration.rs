use super::legacy_warranty::{
    LegacyInvoice, LegacySoldUnit, MigratedWarranty, legacy_profile, legacy_warranty,
};
use crate::clients::db::Db;
use crate::clients::sqlite::parse_iso_datetime;
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::TENANT_FIELD;
use crate::domain::billing::InvoiceItem;
use crate::modules::inventory::service::stock_batch_migration::Mode;
use crate::modules::warranties::repository;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::UpdateOptions;
use sqlx::{Executor, Row, SqlitePool};

/// What a run found (and, unless it was a dry run, did).
#[derive(Debug, Default)]
pub struct Report {
    /// Products given the new warranty profile.
    pub products: u64,
    /// Products whose old month count became a shop warranty.
    pub products_with_warranty: u64,
    /// Warranty records created from old sold serials.
    pub warranties: u64,
    /// Old sold serials that could not be turned into a warranty, and why.
    pub skipped: Vec<String>,
}

async fn columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool)
        .await
}

fn protocol(err: impl std::fmt::Display) -> sqlx::Error {
    sqlx::Error::Protocol(err.to_string())
}

/// Moves an old SQLite database to the warranty records, once: each product's
/// `warranty_months` becomes its `warranty` profile, every sold serial that carried a warranty
/// gets its warranty record, and the old columns are dropped. Runs only after the schema has
/// created `warranties`, and does nothing once `products.warranty_months` is gone. The sync
/// triggers name columns, so they are dropped first (`install_triggers` puts them back) and
/// none of this is uploaded: the cloud migrates itself to the same deterministic keys.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let product_columns = columns(pool, "products").await?;
    if !product_columns.iter().any(|c| c == "warranty_months")
        || columns(pool, "warranties").await?.is_empty()
    {
        return Ok(());
    }
    tracing::info!("Migrating warranties to warranty records");
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(pool)
    .await?;
    for trigger in triggers {
        pool.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }
    if !product_columns.iter().any(|c| c == "warranty") {
        pool.execute(
            "ALTER TABLE products ADD COLUMN warranty TEXT NOT NULL \
             DEFAULT '{\"coverage\":{\"kind\":\"none\"}}'",
        )
        .await?;
    }
    let products = sqlx::query("SELECT key, warranty_months FROM products")
        .fetch_all(pool)
        .await?;
    for row in products {
        let key: String = row.get("key");
        let months: Option<i64> = row.get("warranty_months");
        let profile = serde_json::to_string(&legacy_profile(months))
            .expect("a product warranty serializes to JSON");
        sqlx::query("UPDATE products SET warranty = ? WHERE key = ?")
            .bind(profile)
            .bind(key)
            .execute(pool)
            .await?;
    }

    let serial_columns = columns(pool, "product_serials").await?;
    if serial_columns.iter().any(|c| c == "warranty_months") {
        let serials = sqlx::query(
            "SELECT key, serial_number, product_key, status, invoice_key, sold_at, \
             warranty_months FROM product_serials \
             WHERE warranty_months > 0 AND invoice_key IS NOT NULL AND sold_at IS NOT NULL",
        )
        .fetch_all(pool)
        .await?;
        let db = Db::Sqlite(pool.clone());
        for serial in serials {
            let invoice_key: String = serial.get("invoice_key");
            let Some(invoice) = sqlx::query(
                "SELECT key, invoice_number, status, customer_key, customer_name_snapshot, \
                 customer_phone_snapshot, items FROM invoices WHERE key = ?",
            )
            .bind(&invoice_key)
            .fetch_optional(pool)
            .await?
            else {
                tracing::warn!(
                    invoice_key,
                    "a sold serial's invoice is missing; no warranty"
                );
                continue;
            };
            let items_json: String = invoice.get("items");
            let items: Vec<InvoiceItem> = serde_json::from_str(&items_json).map_err(protocol)?;
            let sold_at: String = serial.get("sold_at");
            let unit = LegacySoldUnit {
                serial_key: serial.get("key"),
                serial_number: serial.get("serial_number"),
                product_key: serial.get("product_key"),
                serial_status: serial.get("status"),
                months: serial.get("warranty_months"),
                sold_at: parse_iso_datetime(&sold_at).map_err(protocol)?,
                invoice: LegacyInvoice {
                    key: invoice.get("key"),
                    number: invoice.get("invoice_number"),
                    status: invoice.get("status"),
                    customer_key: invoice.get("customer_key"),
                    customer_name: invoice.get("customer_name_snapshot"),
                    customer_phone: invoice.get("customer_phone_snapshot"),
                    items: &items,
                },
            };
            let MigratedWarranty { warranty, created } = match legacy_warranty(&unit) {
                Ok(migrated) => migrated,
                Err(reason) => {
                    tracing::warn!(reason, "an old warranty could not be carried over");
                    continue;
                }
            };
            if repository::find_by_key(&db, &warranty.key)
                .await
                .map_err(protocol)?
                .is_none()
            {
                repository::insert_warranty(&db, warranty)
                    .await
                    .map_err(protocol)?;
                repository::insert_event(&db, created)
                    .await
                    .map_err(protocol)?;
            }
        }
        pool.execute("ALTER TABLE product_serials DROP COLUMN warranty_months")
            .await?;
        pool.execute("ALTER TABLE product_serials DROP COLUMN warranty_expires_at")
            .await?;
    }
    pool.execute("ALTER TABLE products DROP COLUMN warranty_months")
        .await?;
    Ok(())
}

fn text<'a>(document: &'a Document, collection: &str, field: &str) -> AppResult<&'a str> {
    document
        .get_str(field)
        .map_err(|e| AppError::internal(format!("{collection}.{field}: {e}")))
}

fn optional_text<'a>(document: &'a Document, field: &str) -> Option<&'a str> {
    document.get_str(field).ok()
}

/// The old month count, stored as either integer width.
fn months_of(document: &Document) -> Option<i64> {
    match document.get("warranty_months") {
        Some(Bson::Int64(m)) => Some(*m),
        Some(Bson::Int32(m)) => Some(i64::from(*m)),
        _ => None,
    }
}

/// Moves every shop on a MongoDB database to the warranty records: products without a
/// `warranty` get the profile their old month count describes (none when they had none), each
/// sold serial that carried a warranty gets its record under the same deterministic key the
/// desktops use, and the old fields are removed. Idempotent: a product that already has a
/// profile, and a warranty that already exists, are left as they are.
pub async fn migrate_mongo(db: &Database, mode: Mode) -> AppResult<Report> {
    let products = db.collection::<Document>("products");
    let serials = db.collection::<Document>("product_serials");
    let invoices = db.collection::<Document>("invoices");
    let warranties = db.collection::<Document>("warranties");
    let events = db.collection::<Document>("warranty_events");
    let mut report = Report::default();

    let mut cursor = products
        .find(doc! { "warranty": { "$exists": false } })
        .await?;
    while let Some(product) = cursor.try_next().await? {
        let months = months_of(&product);
        let profile = legacy_profile(months);
        report.products += 1;
        if profile.is_covered() {
            report.products_with_warranty += 1;
        }
        if mode == Mode::Apply {
            let id = product
                .get_object_id("_id")
                .map_err(|e| AppError::internal(format!("products._id: {e}")))?;
            products
                .update_one(
                    doc! { "_id": id },
                    doc! {
                        "$set": { "warranty": mongodb::bson::serialize_to_document(&profile)? },
                        "$unset": { "warranty_months": "" },
                    },
                )
                .await?;
        }
    }

    let mut cursor = serials
        .find(doc! {
            "warranty_months": { "$gt": 0 },
            "invoice_key": { "$exists": true },
            "sold_at": { "$exists": true },
        })
        .await?;
    while let Some(serial) = cursor.try_next().await? {
        let tenant = optional_text(&serial, TENANT_FIELD);
        let invoice_key = text(&serial, "product_serials", "invoice_key")?;
        let mut invoice_filter = doc! { "key": invoice_key };
        if let Some(tenant) = tenant {
            invoice_filter.insert(TENANT_FIELD, tenant);
        }
        let Some(invoice) = invoices.find_one(invoice_filter).await? else {
            report
                .skipped
                .push(format!("invoice {invoice_key} of a sold serial is missing"));
            continue;
        };
        let items: Vec<InvoiceItem> = mongodb::bson::deserialize_from_bson(
            invoice
                .get("items")
                .cloned()
                .ok_or_else(|| AppError::internal("invoices.items is missing"))?,
        )?;
        let sold_at = serial
            .get_datetime("sold_at")
            .map_err(|e| AppError::internal(format!("product_serials.sold_at: {e}")))?
            .to_chrono();
        let unit = LegacySoldUnit {
            serial_key: text(&serial, "product_serials", "key")?,
            serial_number: text(&serial, "product_serials", "serial_number")?,
            product_key: text(&serial, "product_serials", "product_key")?,
            serial_status: text(&serial, "product_serials", "status")?,
            months: months_of(&serial)
                .ok_or_else(|| AppError::internal("product_serials.warranty_months"))?,
            sold_at,
            invoice: LegacyInvoice {
                key: text(&invoice, "invoices", "key")?,
                number: text(&invoice, "invoices", "invoice_number")?,
                status: text(&invoice, "invoices", "status")?,
                customer_key: optional_text(&invoice, "customer_key"),
                customer_name: optional_text(&invoice, "customer_name_snapshot"),
                customer_phone: optional_text(&invoice, "customer_phone_snapshot"),
                items: &items,
            },
        };
        let MigratedWarranty { warranty, created } = match legacy_warranty(&unit) {
            Ok(migrated) => migrated,
            Err(reason) => {
                report.skipped.push(reason);
                continue;
            }
        };
        report.warranties += 1;
        if mode == Mode::Apply {
            for (collection, key, document) in [
                (
                    &warranties,
                    warranty.key.clone(),
                    mongodb::bson::serialize_to_document(&warranty)?,
                ),
                (
                    &events,
                    created.key.clone(),
                    mongodb::bson::serialize_to_document(&created)?,
                ),
            ] {
                let mut fresh = document;
                let mut filter = doc! { "key": &key };
                if let Some(tenant) = tenant {
                    fresh.insert(TENANT_FIELD, tenant);
                    filter.insert(TENANT_FIELD, tenant);
                }
                collection
                    .update_one(filter, doc! { "$setOnInsert": fresh })
                    .with_options(UpdateOptions::builder().upsert(true).build())
                    .await?;
            }
        }
    }
    if mode == Mode::Apply {
        serials
            .update_many(
                doc! { "$or": [
                    { "warranty_months": { "$exists": true } },
                    { "warranty_expires_at": { "$exists": true } },
                ] },
                doc! { "$unset": { "warranty_months": "", "warranty_expires_at": "" } },
            )
            .await?;
    }
    Ok(report)
}
