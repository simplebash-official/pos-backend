use super::product_serials::product_serials;
use super::row_to_serial_doc::row_to_serial_doc;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use crate::modules::inventory::model::ProductSerialDocument;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

/// Atomically transitions a serial's status and stamps whichever of
/// `invoice_key`/`sold_at`/`warranty_expires_at`/`credit_note_key` apply to
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
            if let Ok(wm) = set_doc.get_i64("warranty_months") {
                doc.warranty_months = Some(wm);
            }
            if let Ok(we) = set_doc.get_datetime("warranty_expires_at") {
                doc.warranty_expires_at = Some(*we);
            }
            if let Ok(cnk) = set_doc.get_str("credit_note_key") {
                doc.credit_note_key = Some(cnk.to_string());
            }
            doc.updated_at = mongodb::bson::DateTime::now();
            doc.version += 1;

            let sold_at_iso = doc.sold_at.as_ref().map(bson_to_iso);
            let warranty_expires_at_iso = doc.warranty_expires_at.as_ref().map(bson_to_iso);
            let updated_at_iso = bson_to_iso(&doc.updated_at);

            sqlx::query(
                r#"
                UPDATE product_serials SET
                    status = ?, invoice_key = ?, sold_at = ?, warranty_months = ?,
                    warranty_expires_at = ?, credit_note_key = ?, version = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(doc.status.as_str())
            .bind(&doc.invoice_key)
            .bind(&sold_at_iso)
            .bind(doc.warranty_months)
            .bind(&warranty_expires_at_iso)
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
