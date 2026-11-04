use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::{AppError, AppResult};
use crate::modules::warranties::model::{WarrantyDocument, WarrantyEventDocument};
use futures_util::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

const WARRANTIES: &str = "warranties";
const EVENTS: &str = "warranty_events";

const WARRANTY_COLUMNS: &str = "key, id, warranty_number, source_type, invoice_key, \
     invoice_number, line_index, product_key, repair_key, repair_item_key, item_name, \
     serial_number, quantity, customer_key, customer_name, customer_phone, customer_phone_tail, \
     coverage, warranty_type, terms, unit_price_cents, starts_on, ends_on, status, void_reason, \
     replaces_warranty_key, replaced_by_warranty_key, claim_count, version, created_at, \
     updated_at, updated_by_device";

const EVENT_COLUMNS: &str = "key, id, warranty_key, claim_key, kind, from_status, to_status, \
     note, actor_user_id, actor_name, approved_by_user_id, approved_by_name, version, \
     created_at, updated_at, updated_by_device";

fn warranties(db: &TenantDatabase) -> ScopedCollection<WarrantyDocument> {
    db.collection(WARRANTIES)
}

fn events(db: &TenantDatabase) -> ScopedCollection<WarrantyEventDocument> {
    db.collection(EVENTS)
}

/// A serde enum's stored text.
fn enum_text<T: serde::Serialize>(value: &T) -> AppResult<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => Ok(text),
        other => Err(AppError::internal(format!(
            "warranty enum did not serialize to text: {other:?}"
        ))),
    }
}

/// Reads a stored serde enum from its text.
fn parse_enum<T: serde::de::DeserializeOwned>(field: &str, text: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(text.to_string()))
        .map_err(|_| AppError::internal(format!("stored warranty {field} `{text}` is unknown")))
}

fn row_to_warranty(row: &SqliteRow) -> AppResult<WarrantyDocument> {
    let id: String = row.get("id");
    let coverage: String = row.get("coverage");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(WarrantyDocument {
        id: Some(
            ObjectId::parse_str(&id)
                .map_err(|_| AppError::internal(format!("stored warranty id `{id}` is invalid")))?,
        ),
        key: row.get("key"),
        warranty_number: row.get("warranty_number"),
        source_type: parse_enum("source_type", row.get("source_type"))?,
        invoice_key: row.get("invoice_key"),
        invoice_number: row.get("invoice_number"),
        line_index: row.get("line_index"),
        product_key: row.get("product_key"),
        repair_key: row.get("repair_key"),
        repair_item_key: row.get("repair_item_key"),
        item_name: row.get("item_name"),
        serial_number: row.get("serial_number"),
        quantity: row.get("quantity"),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        customer_phone_tail: row.get("customer_phone_tail"),
        coverage: serde_json::from_str(&coverage).map_err(|err| {
            AppError::internal(format!("stored warranty coverage is not readable: {err}"))
        })?,
        warranty_type: parse_enum("warranty_type", row.get("warranty_type"))?,
        terms: row.get("terms"),
        unit_price_cents: row.get("unit_price_cents"),
        starts_on: row.get("starts_on"),
        ends_on: row.get("ends_on"),
        status: parse_enum("status", row.get("status"))?,
        void_reason: row.get("void_reason"),
        replaces_warranty_key: row.get("replaces_warranty_key"),
        replaced_by_warranty_key: row.get("replaced_by_warranty_key"),
        claim_count: row.get("claim_count"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at)?,
        updated_at: to_bson_datetime(&updated_at)?,
        updated_by_device: row.get("updated_by_device"),
    })
}

fn row_to_event(row: &SqliteRow) -> AppResult<WarrantyEventDocument> {
    let id: String = row.get("id");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(WarrantyEventDocument {
        id: Some(ObjectId::parse_str(&id).map_err(|_| {
            AppError::internal(format!("stored warranty event id `{id}` is invalid"))
        })?),
        key: row.get("key"),
        warranty_key: row.get("warranty_key"),
        claim_key: row.get("claim_key"),
        kind: parse_enum("event kind", row.get("kind"))?,
        from_status: row.get("from_status"),
        to_status: row.get("to_status"),
        note: row.get("note"),
        actor_user_id: row.get("actor_user_id"),
        actor_name: row.get("actor_name"),
        approved_by_user_id: row.get("approved_by_user_id"),
        approved_by_name: row.get("approved_by_name"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at)?,
        updated_at: to_bson_datetime(&updated_at)?,
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Collects a Mongo cursor of warranties.
async fn collect<T: serde::de::DeserializeOwned + Send + Sync + Unpin>(
    cursor: mongodb::Cursor<T>,
) -> AppResult<Vec<T>> {
    Ok(cursor.try_collect().await?)
}

/// Stores a new warranty. Returns it with its id.
pub(crate) async fn insert_warranty(
    db: &Db,
    mut warranty: WarrantyDocument,
) -> AppResult<WarrantyDocument> {
    match db {
        Db::Mongo(mongo) => {
            let result = warranties(mongo).insert_one(&warranty).await?;
            warranty.id = result.inserted_id.as_object_id();
            Ok(warranty)
        }
        Db::Sqlite(pool) => {
            let id = warranty.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            let coverage = serde_json::to_string(&warranty.coverage)
                .expect("a warranty coverage serializes to JSON");
            sqlx::query(&format!(
                "INSERT INTO warranties ({WARRANTY_COLUMNS}) VALUES \
                 (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                  ?, ?, ?, ?, ?)"
            ))
            .bind(&warranty.key)
            .bind(&id)
            .bind(&warranty.warranty_number)
            .bind(enum_text(&warranty.source_type)?)
            .bind(&warranty.invoice_key)
            .bind(&warranty.invoice_number)
            .bind(warranty.line_index)
            .bind(&warranty.product_key)
            .bind(&warranty.repair_key)
            .bind(&warranty.repair_item_key)
            .bind(&warranty.item_name)
            .bind(&warranty.serial_number)
            .bind(warranty.quantity)
            .bind(&warranty.customer_key)
            .bind(&warranty.customer_name)
            .bind(&warranty.customer_phone)
            .bind(&warranty.customer_phone_tail)
            .bind(&coverage)
            .bind(enum_text(&warranty.warranty_type)?)
            .bind(&warranty.terms)
            .bind(warranty.unit_price_cents)
            .bind(&warranty.starts_on)
            .bind(&warranty.ends_on)
            .bind(enum_text(&warranty.status)?)
            .bind(&warranty.void_reason)
            .bind(&warranty.replaces_warranty_key)
            .bind(&warranty.replaced_by_warranty_key)
            .bind(warranty.claim_count)
            .bind(warranty.version)
            .bind(bson_to_iso(&warranty.created_at))
            .bind(bson_to_iso(&warranty.updated_at))
            .bind(&warranty.updated_by_device)
            .execute(pool)
            .await?;
            warranty.id = Some(
                ObjectId::parse_str(&id).expect("generate_id_hex returns an ObjectId hex string"),
            );
            Ok(warranty)
        }
    }
}

/// Writes a changed warranty over the stored one, only if nobody changed it since it was read
/// (`version` must still be `expected_version`). `false` when it had changed.
pub(crate) async fn replace_warranty(
    db: &Db,
    warranty: &WarrantyDocument,
    expected_version: i64,
) -> AppResult<bool> {
    match db {
        Db::Mongo(mongo) => Ok(warranties(mongo)
            .replace_one(
                doc! { "key": &warranty.key, "version": expected_version },
                warranty,
            )
            .await?
            .matched_count
            == 1),
        Db::Sqlite(pool) => {
            let coverage = serde_json::to_string(&warranty.coverage)
                .expect("a warranty coverage serializes to JSON");
            let result = sqlx::query(
                "UPDATE warranties SET coverage = ?, warranty_type = ?, terms = ?, ends_on = ?, \
                 status = ?, void_reason = ?, replaces_warranty_key = ?, \
                 replaced_by_warranty_key = ?, quantity = ?, claim_count = ?, version = ?, \
                 updated_at = ? WHERE key = ? AND version = ?",
            )
            .bind(&coverage)
            .bind(enum_text(&warranty.warranty_type)?)
            .bind(&warranty.terms)
            .bind(&warranty.ends_on)
            .bind(enum_text(&warranty.status)?)
            .bind(&warranty.void_reason)
            .bind(&warranty.replaces_warranty_key)
            .bind(&warranty.replaced_by_warranty_key)
            .bind(warranty.quantity)
            .bind(warranty.claim_count)
            .bind(warranty.version)
            .bind(bson_to_iso(&warranty.updated_at))
            .bind(&warranty.key)
            .bind(expected_version)
            .execute(pool)
            .await?;
            Ok(result.rows_affected() == 1)
        }
    }
}

/// One warranty by its `wty_` key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<WarrantyDocument>> {
    match db {
        Db::Mongo(mongo) => Ok(warranties(mongo).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => sqlx::query(&format!(
            "SELECT {WARRANTY_COLUMNS} FROM warranties WHERE key = ?"
        ))
        .bind(key)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_warranty)
        .transpose(),
    }
}

/// Which warranties a query wants, as a Mongo filter and the SQLite `WHERE` with its values.
#[derive(Debug)]
pub(crate) enum WarrantyFilter<'a> {
    /// The exact warranty number (case as stored).
    Number(&'a str),
    /// The exact serial or IMEI.
    Serial(&'a str),
    /// Serials starting with this text.
    SerialPrefix(&'a str),
    /// Every warranty on the invoice with this number.
    InvoiceNumber(&'a str),
    /// Every warranty on this invoice.
    InvoiceKey(&'a str),
    /// The customer's phone, by its last nine digits.
    PhoneTail(&'a str),
    /// One customer.
    Customer(&'a str),
    /// The items of one repair job.
    Repair(&'a str),
    /// Everything.
    All,
}

impl WarrantyFilter<'_> {
    fn mongo(&self) -> Document {
        match self {
            WarrantyFilter::Number(n) => doc! { "warranty_number": *n },
            WarrantyFilter::Serial(s) => doc! { "serial_number": *s },
            WarrantyFilter::SerialPrefix(p) => doc! {
                "serial_number": {
                    "$regex": format!("^{}", crate::core::utils::regex_escape(p)),
                }
            },
            WarrantyFilter::InvoiceNumber(n) => doc! { "invoice_number": *n },
            WarrantyFilter::InvoiceKey(k) => doc! { "invoice_key": *k },
            WarrantyFilter::PhoneTail(t) => doc! { "customer_phone_tail": *t },
            WarrantyFilter::Customer(k) => doc! { "customer_key": *k },
            WarrantyFilter::Repair(k) => doc! { "repair_key": *k },
            WarrantyFilter::All => doc! {},
        }
    }

    fn sqlite(&self) -> (&'static str, Option<String>) {
        match self {
            WarrantyFilter::Number(n) => ("warranty_number = ?", Some((*n).to_string())),
            WarrantyFilter::Serial(s) => ("serial_number = ?", Some((*s).to_string())),
            // `instr` keeps `%` and `_` in a serial literal, unlike LIKE.
            WarrantyFilter::SerialPrefix(p) => {
                ("instr(serial_number, ?) = 1", Some((*p).to_string()))
            }
            WarrantyFilter::InvoiceNumber(n) => ("invoice_number = ?", Some((*n).to_string())),
            WarrantyFilter::InvoiceKey(k) => ("invoice_key = ?", Some((*k).to_string())),
            WarrantyFilter::PhoneTail(t) => ("customer_phone_tail = ?", Some((*t).to_string())),
            WarrantyFilter::Customer(k) => ("customer_key = ?", Some((*k).to_string())),
            WarrantyFilter::Repair(k) => ("repair_key = ?", Some((*k).to_string())),
            WarrantyFilter::All => ("1 = 1", None),
        }
    }
}

/// A page of warranties matching `filter`, newest first, and how many match in all.
pub(crate) async fn list(
    db: &Db,
    filter: &WarrantyFilter<'_>,
    skip: u64,
    limit: u64,
) -> AppResult<(Vec<WarrantyDocument>, u64)> {
    match db {
        Db::Mongo(mongo) => {
            let collection = warranties(mongo);
            let total = collection.count_documents(filter.mongo()).await?;
            let cursor = collection
                .find(filter.mongo())
                .sort(doc! { "created_at": -1, "key": -1 })
                .skip(skip)
                .limit(i64::try_from(limit).expect("page limits are small"))
                .await?;
            Ok((collect(cursor).await?, total))
        }
        Db::Sqlite(pool) => {
            let (clause, value) = filter.sqlite();
            let count_sql = format!("SELECT COUNT(*) FROM warranties WHERE {clause}");
            let mut count = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(value) = &value {
                count = count.bind(value);
            }
            let total =
                u64::try_from(count.fetch_one(pool).await?).expect("a row count is never negative");
            let select = format!(
                "SELECT {WARRANTY_COLUMNS} FROM warranties WHERE {clause} \
                 ORDER BY created_at DESC, key DESC LIMIT ? OFFSET ?"
            );
            let mut query = sqlx::query(&select);
            if let Some(value) = &value {
                query = query.bind(value);
            }
            let rows = query
                .bind(i64::try_from(limit).expect("page limits are small"))
                .bind(i64::try_from(skip).expect("page offsets are small"))
                .fetch_all(pool)
                .await?;
            Ok((
                rows.iter().map(row_to_warranty).collect::<AppResult<_>>()?,
                total,
            ))
        }
    }
}

/// Appends one line to a warranty's activity log.
pub(crate) async fn insert_event(db: &Db, mut event: WarrantyEventDocument) -> AppResult<()> {
    match db {
        Db::Mongo(mongo) => {
            let result = events(mongo).insert_one(&event).await?;
            event.id = result.inserted_id.as_object_id();
            Ok(())
        }
        Db::Sqlite(pool) => {
            let id = event.id.map_or_else(generate_id_hex, |oid| oid.to_hex());
            sqlx::query(&format!(
                "INSERT INTO warranty_events ({EVENT_COLUMNS}) VALUES \
                 (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            ))
            .bind(&event.key)
            .bind(&id)
            .bind(&event.warranty_key)
            .bind(&event.claim_key)
            .bind(enum_text(&event.kind)?)
            .bind(&event.from_status)
            .bind(&event.to_status)
            .bind(&event.note)
            .bind(&event.actor_user_id)
            .bind(&event.actor_name)
            .bind(&event.approved_by_user_id)
            .bind(&event.approved_by_name)
            .bind(event.version)
            .bind(bson_to_iso(&event.created_at))
            .bind(bson_to_iso(&event.updated_at))
            .bind(&event.updated_by_device)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}

/// A warranty's activity log, oldest first.
pub(crate) async fn list_events(
    db: &Db,
    warranty_key: &str,
) -> AppResult<Vec<WarrantyEventDocument>> {
    match db {
        Db::Mongo(mongo) => {
            let cursor = events(mongo)
                .find(doc! { "warranty_key": warranty_key })
                .sort(doc! { "created_at": 1, "key": 1 })
                .await?;
            collect(cursor).await
        }
        Db::Sqlite(pool) => sqlx::query(&format!(
            "SELECT {EVENT_COLUMNS} FROM warranty_events WHERE warranty_key = ? \
             ORDER BY created_at ASC, key ASC"
        ))
        .bind(warranty_key)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row_to_event)
        .collect(),
    }
}
