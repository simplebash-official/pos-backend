use super::find_supplier_by_id::find_supplier_by_id;
use super::suppliers::suppliers;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;

use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn delete_supplier(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! {
                "deleted_at": now,
                "updated_at": now,
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(suppliers(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": set_doc,
                        "$inc": { "version": 1 }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_supplier_by_id(db, id).await?;
            let Some(mut sup) = existing else {
                return Ok(None);
            };

            let now = BsonDateTime::now();
            sup.deleted_at = Some(now);
            sup.updated_at = now;
            sup.version += 1;
            sup.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE suppliers SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(sup))
        }
    }
}
