// Database access for the `stock_batches` collection only — one row per
// delivery of a product, with its own cost, selling price and remaining
// quantity. `remaining_quantity` is a derived column (the sum of the stock
// movements carrying the batch's key): it only moves through
// `adjust_batch_remaining`, which `service::stock::apply_stock_delta` calls
// together with the movement insert, and never bumps `version`/`updated_at`
// (same rule as `products.stock_quantity`).

use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        uow::{Backend, Unit},
    },
    core::error::AppResult,
    modules::inventory::model::StockBatchDocument,
};

fn stock_batches(db: &TenantDatabase) -> ScopedCollection<StockBatchDocument> {
    db.collection("stock_batches")
}

fn row_to_batch_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<StockBatchDocument> {
    let id_str: String = row.get("id");
    let source_str: String = row.get("source");
    let received_at_str: String = row.get("received_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(StockBatchDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        product_key: row.get("product_key"),
        source: source_str.parse()?,
        purchase_key: row.get("purchase_key"),
        supplier_key: row.get("supplier_key"),
        received_quantity: row.get("received_quantity"),
        remaining_quantity: row.get("remaining_quantity"),
        unit_cost_cents: row.get("unit_cost_cents"),
        selling_price_cents: row.get("selling_price_cents"),
        received_at: to_bson_datetime(&received_at_str),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a new batch and returns it with its id filled in.
pub(crate) async fn insert_batch(
    unit: impl Into<Unit<'_>>,
    mut batch: StockBatchDocument,
) -> AppResult<StockBatchDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = stock_batches(db).insert_one_in(&batch, session).await?;
            batch.id = result.inserted_id.as_object_id();
            Ok(batch)
        }
        Backend::Sqlite(mut conn) => {
            let id = batch
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                r#"
                INSERT INTO stock_batches (
                    key, id, product_key, source, purchase_key, supplier_key,
                    received_quantity, remaining_quantity, unit_cost_cents,
                    selling_price_cents, received_at, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&batch.key)
            .bind(&id)
            .bind(&batch.product_key)
            .bind(batch.source.as_str())
            .bind(&batch.purchase_key)
            .bind(&batch.supplier_key)
            .bind(batch.received_quantity)
            .bind(batch.remaining_quantity)
            .bind(batch.unit_cost_cents)
            .bind(batch.selling_price_cents)
            .bind(bson_to_iso(&batch.received_at))
            .bind(batch.version)
            .bind(bson_to_iso(&batch.created_at))
            .bind(bson_to_iso(&batch.updated_at))
            .bind(batch.deleted_at.as_ref().map(bson_to_iso))
            .bind(&batch.updated_by_device)
            .execute(&mut *conn)
            .await?;
            batch.id = ObjectId::parse_str(&id).ok();
            Ok(batch)
        }
    }
}

/// Fetches one live batch by its key.
pub(crate) async fn find_batch_by_key(
    unit: impl Into<Unit<'_>>,
    key: &str,
) -> AppResult<Option<StockBatchDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => Ok(crate::in_session!(
            stock_batches(db).find_one(doc! { "key": key, "deleted_at": { "$exists": false } }),
            session
        )?),
        Backend::Sqlite(mut conn) => {
            let row =
                sqlx::query("SELECT * FROM stock_batches WHERE key = ? AND deleted_at IS NULL")
                    .bind(key)
                    .fetch_optional(&mut *conn)
                    .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}

/// Every live batch of one product, newest first — the intake history shown
/// on the product drawer, empty batches included.
pub(crate) async fn list_batches_for_product(
    db: &Db,
    product_key: &str,
) -> AppResult<Vec<StockBatchDocument>> {
    match db {
        Db::Mongo(db) => {
            let cursor = stock_batches(db)
                .find(doc! { "product_key": product_key, "deleted_at": { "$exists": false } })
                .sort(doc! { "received_at": -1, "created_at": -1, "key": -1 })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM stock_batches WHERE product_key = ? AND deleted_at IS NULL \
                 ORDER BY received_at DESC, created_at DESC, key DESC",
            )
            .bind(product_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(row_to_batch_doc).collect()
        }
    }
}

/// The live batches of the given products that still hold units, oldest
/// first — the order a sale from a price tier consumes them in, and the input
/// to every product's `price_tiers`.
pub(crate) async fn list_in_stock_batches_for_products(
    db: &Db,
    product_keys: &[String],
) -> AppResult<Vec<StockBatchDocument>> {
    if product_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let cursor = stock_batches(db)
                .find(doc! {
                    "product_key": { "$in": product_keys },
                    "remaining_quantity": { "$gt": 0 },
                    "deleted_at": { "$exists": false },
                })
                .sort(doc! { "received_at": 1, "created_at": 1, "key": 1 })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; product_keys.len()].join(",");
            let sql = format!(
                "SELECT * FROM stock_batches WHERE product_key IN ({placeholders}) \
                 AND remaining_quantity > 0 AND deleted_at IS NULL \
                 ORDER BY received_at ASC, created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in product_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(row_to_batch_doc).collect()
        }
    }
}

/// Moves a batch's derived `remaining_quantity` by `delta` in one atomic
/// write, refusing (returning `None`) when that would take it below zero —
/// the guard and the write are one statement, so two concurrent sales of
/// the last unit cannot both pass. Like the product stock column, only the
/// derived value changes: no `version`/`updated_at` bump, writer preserved.
pub(crate) async fn adjust_batch_remaining(
    unit: impl Into<Unit<'_>>,
    key: &str,
    delta: i64,
) -> AppResult<Option<StockBatchDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let mut filter = doc! { "key": key, "deleted_at": { "$exists": false } };
            if delta < 0 {
                filter.insert("remaining_quantity", doc! { "$gte": -delta });
            }
            let update = vec![doc! {
                "$set": { "remaining_quantity": { "$add": ["$remaining_quantity", delta] } }
            }];
            Ok(crate::in_session!(
                stock_batches(db)
                    .preserve_origin()
                    .find_one_and_update(filter, update)
                    .return_document(ReturnDocument::After),
                session
            )?)
        }
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query(
                r#"
                UPDATE stock_batches
                SET remaining_quantity = remaining_quantity + ?
                WHERE key = ? AND deleted_at IS NULL AND remaining_quantity + ? >= 0
                RETURNING *
                "#,
            )
            .bind(delta)
            .bind(key)
            .bind(delta)
            .fetch_optional(&mut *conn)
            .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}

/// Applies an edit of a batch's own (non-derived) fields — selling price
/// and/or cost — bumping `version`/`updated_at` so the change travels
/// last-writer-wins like any other edit.
pub(crate) async fn update_batch(
    db: &Db,
    key: &str,
    selling_price_cents: Option<i64>,
    unit_cost_cents: Option<i64>,
) -> AppResult<Option<StockBatchDocument>> {
    let now = BsonDateTime::now();
    match db {
        Db::Mongo(db) => {
            let mut set: Document = doc! { "updated_at": now };
            if let Some(price) = selling_price_cents {
                set.insert("selling_price_cents", price);
            }
            if let Some(cost) = unit_cost_cents {
                set.insert("unit_cost_cents", cost);
            }
            Ok(stock_batches(db)
                .find_one_and_update(
                    doc! { "key": key, "deleted_at": { "$exists": false } },
                    doc! { "$set": set, "$inc": { "version": 1 } },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"
                UPDATE stock_batches
                SET selling_price_cents = COALESCE(?, selling_price_cents),
                    unit_cost_cents = COALESCE(?, unit_cost_cents),
                    updated_at = ?, version = version + 1
                WHERE key = ? AND deleted_at IS NULL
                RETURNING *
                "#,
            )
            .bind(selling_price_cents)
            .bind(unit_cost_cents)
            .bind(bson_to_iso(&now))
            .bind(key)
            .fetch_optional(pool)
            .await?;
            row.map(|r| row_to_batch_doc(&r)).transpose()
        }
    }
}
