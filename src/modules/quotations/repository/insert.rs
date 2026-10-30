use super::quotations::quotations;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::{AppError, AppResult};

use crate::modules::quotations::model::QuotationDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert(
    db: &Db,
    mut document: QuotationDocument,
) -> AppResult<QuotationDocument> {
    match db {
        Db::Mongo(db) => {
            let result = quotations(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let items_json = serde_json::to_string(&document.items)
                .map_err(|e| AppError::internal(format!("Could not store quotation items: {e}")))?;
            let snapshot_json =
                serde_json::to_string(&document.shop_profile_snapshot).map_err(|e| {
                    AppError::internal(format!("Could not store quotation shop profile: {e}"))
                })?;

            sqlx::query(
                r#"
                INSERT INTO quotations (
                    key, id, quotation_number, status, has_out_of_stock_items, customer_key, customer_name,
                    customer_phone, customer_address, items, subtotal_cents, discount_type,
                    discount_value, discount_cents, total_cents, valid_until, notes, terms,
                    prepared_by, shop_profile_snapshot, version, created_at, updated_at,
                    deleted_at, updated_by_device, fulfilled_by_invoice_key, fulfilled_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.quotation_number)
            .bind(&document.status)
            .bind(if document.has_out_of_stock_items { 1i32 } else { 0i32 })
            .bind(&document.customer_key)
            .bind(&document.customer_name)
            .bind(&document.customer_phone)
            .bind(&document.customer_address)
            .bind(&items_json)
            .bind(document.subtotal_cents)
            .bind(&document.discount_type)
            .bind(document.discount_value)
            .bind(document.discount_cents)
            .bind(document.total_cents)
            .bind(&document.valid_until)
            .bind(&document.notes)
            .bind(&document.terms)
            .bind(&document.prepared_by)
            .bind(&snapshot_json)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(document.deleted_at.as_ref().map(bson_to_iso))
            .bind(&document.updated_by_device)
            .bind(&document.fulfilled_by_invoice_key)
            .bind(document.fulfilled_at.as_ref().map(bson_to_iso))
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
