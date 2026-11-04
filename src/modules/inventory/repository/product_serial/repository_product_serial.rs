use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::{AppError, AppResult};
use crate::domain::inventory::SerialStatus;
use crate::modules::inventory::model::ProductSerialDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(crate) async fn find_serial_by_number(
    db: &Db,
    serial_number: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => Ok(product_serials(db)
            .find_one(doc! { "serial_number": serial_number })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM product_serials WHERE serial_number = ?")
                .bind(serial_number)
                .fetch_optional(pool)
                .await?;

            row.map(|r| row_to_serial_doc(&r)).transpose()
        }
    }
}

/// Looks up a serial by number, additionally scoped to a specific product —
/// used when validating a sale/return references a serial that actually
/// belongs to the product line it's attached to.
pub(crate) async fn find_serial_by_number_and_product(
    db: &Db,
    serial_number: &str,
    product_key: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => Ok(product_serials(db)
            .find_one(doc! { "serial_number": serial_number, "product_key": product_key })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                "SELECT * FROM product_serials WHERE serial_number = ? AND product_key = ?",
            )
            .bind(serial_number)
            .bind(product_key)
            .fetch_optional(pool)
            .await?;

            row.map(|r| row_to_serial_doc(&r)).transpose()
        }
    }
}

pub(crate) async fn find_serials_for_product(
    db: &Db,
    product_key: &str,
    status: Option<&str>,
) -> AppResult<Vec<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut filter = doc! { "product_key": product_key };
            if let Some(status) = status {
                filter.insert("status", status);
            }
            let mut cursor = product_serials(db)
                .find(filter)
                .sort(doc! { "serial_number": 1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = if let Some(st) = status {
                sqlx::query(
                    "SELECT * FROM product_serials WHERE product_key = ? AND status = ? ORDER BY serial_number ASC",
                )
                .bind(product_key)
                .bind(st)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT * FROM product_serials WHERE product_key = ? ORDER BY serial_number ASC",
                )
                .bind(product_key)
                .fetch_all(pool)
                .await?
            };

            rows.iter().map(row_to_serial_doc).collect()
        }
    }
}

pub(crate) async fn insert_product_serial(
    unit: impl Into<Unit<'_>>,
    serial: ProductSerialDocument,
) -> AppResult<ProductSerialDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = product_serials(db).insert_one_in(&serial, session).await?;
            let mut inserted = serial;
            inserted.id = result.inserted_id.as_object_id();
            Ok(inserted)
        }
        Backend::Sqlite(mut conn) => {
            let id = serial
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let sold_at_iso = serial.sold_at.as_ref().map(bson_to_iso);
            let created_at_iso = bson_to_iso(&serial.created_at);
            let updated_at_iso = bson_to_iso(&serial.updated_at);

            sqlx::query(
                r#"
                INSERT INTO product_serials (
                    key, id, product_key, serial_number, batch_key, status, invoice_key,
                    sold_at, credit_note_key, version, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&serial.key)
            .bind(&id)
            .bind(&serial.product_key)
            .bind(&serial.serial_number)
            .bind(&serial.batch_key)
            .bind(serial.status.as_str())
            .bind(&serial.invoice_key)
            .bind(&sold_at_iso)
            .bind(&serial.credit_note_key)
            .bind(serial.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(&mut *conn)
            .await?;

            let mut inserted = serial;
            inserted.id = ObjectId::parse_str(&id).ok();
            Ok(inserted)
        }
    }
}

pub(super) fn product_serials(db: &TenantDatabase) -> ScopedCollection<ProductSerialDocument> {
    db.collection("product_serials")
}

pub(super) fn row_to_serial_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProductSerialDocument> {
    let id_str: String = row.get("id");
    let object_id = ObjectId::parse_str(&id_str).ok();
    let status_str: String = row.get("status");
    let status: SerialStatus = status_str.parse().map_err(|_| {
        AppError::internal(format!("stored serial status `{status_str}` is unknown"))
    })?;
    let sold_at_str: Option<String> = row.get("sold_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");

    Ok(ProductSerialDocument {
        id: object_id,
        key: row.get("key"),
        product_key: row.get("product_key"),
        serial_number: row.get("serial_number"),
        batch_key: row.get("batch_key"),
        status,
        invoice_key: row.get("invoice_key"),
        sold_at: sold_at_str.map(|s| to_bson_datetime(&s)).transpose()?,
        credit_note_key: row.get("credit_note_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str)?,
        updated_at: to_bson_datetime(&updated_at_str)?,
    })
}

/// Puts a sold unit back on the shelf: status `in_stock`, and the sale's
/// `invoice_key`/`sold_at` cleared. `None` when no serial
/// has this id.
pub(crate) async fn release_serial(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => Ok(product_serials(db)
            .find_one_and_update(
                doc! { "_id": id },
                doc! {
                    "$set": {
                        "status": SerialStatus::InStock.as_str(),
                        "updated_at": mongodb::bson::DateTime::now(),
                    },
                    "$unset": { "invoice_key": "", "sold_at": "" },
                },
            )
            .return_document(mongodb::options::ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let updated_at_iso = bson_to_iso(&mongodb::bson::DateTime::now());
            let row = sqlx::query(
                r#"
                UPDATE product_serials SET
                    status = ?, invoice_key = NULL, sold_at = NULL, version = version + 1,
                    updated_at = ?
                WHERE id = ?
                RETURNING *
                "#,
            )
            .bind(SerialStatus::InStock.as_str())
            .bind(&updated_at_iso)
            .bind(id.to_hex())
            .fetch_optional(pool)
            .await?;
            row.map(|r| row_to_serial_doc(&r)).transpose()
        }
    }
}

/// Atomically transitions a serial's status and stamps whichever of
/// `invoice_key`/`sold_at`/`credit_note_key` apply to
/// that transition — callers pass only the fields relevant to their
/// transition, everything else in `set_doc` is left untouched.
pub(crate) async fn update_serial(
    db: &Db,
    id: ObjectId,
    set_doc: mongodb::bson::Document,
) -> AppResult<Option<ProductSerialDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut update = set_doc;
            update.insert("updated_at", mongodb::bson::DateTime::now());
            Ok(product_serials(db)
                .find_one_and_update(doc! { "_id": id }, doc! { "$set": update })
                .return_document(mongodb::options::ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing_row = sqlx::query("SELECT * FROM product_serials WHERE id = ?")
                .bind(&id_str)
                .fetch_optional(pool)
                .await?;

            let Some(row) = existing_row else {
                return Ok(None);
            };

            let mut doc = row_to_serial_doc(&row)?;

            if let Ok(st_str) = set_doc.get_str("status")
                && let Ok(parsed_st) = st_str.parse()
            {
                doc.status = parsed_st;
            }
            if let Ok(ik) = set_doc.get_str("invoice_key") {
                doc.invoice_key = Some(ik.to_string());
            }
            if let Ok(sa) = set_doc.get_datetime("sold_at") {
                doc.sold_at = Some(*sa);
            }
            if let Ok(cnk) = set_doc.get_str("credit_note_key") {
                doc.credit_note_key = Some(cnk.to_string());
            }
            doc.updated_at = mongodb::bson::DateTime::now();
            doc.version += 1;

            let sold_at_iso = doc.sold_at.as_ref().map(bson_to_iso);
            let updated_at_iso = bson_to_iso(&doc.updated_at);

            sqlx::query(
                r#"
                UPDATE product_serials SET
                    status = ?, invoice_key = ?, sold_at = ?, credit_note_key = ?, version = ?,
                    updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(doc.status.as_str())
            .bind(&doc.invoice_key)
            .bind(&sold_at_iso)
            .bind(&doc.credit_note_key)
            .bind(doc.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(doc))
        }
    }
}
