use super::customers::customers;
use super::find_customer_by_id::find_customer_by_id;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_customer(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<CustomerDocument>> {
    match db {
        Db::Mongo(db) => Ok(customers(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_customer_by_id(db, id).await?;
            let Some(mut cust) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                cust.name = name.to_string();
            }
            if set_doc.contains_key("contact_person") {
                cust.contact_person = set_doc
                    .get_str("contact_person")
                    .ok()
                    .map(|s| s.to_string());
            }
            if let Ok(phone) = set_doc.get_str("primary_phone") {
                cust.primary_phone = phone.to_string();
            }
            if set_doc.contains_key("secondary_phone") {
                cust.secondary_phone = set_doc
                    .get_str("secondary_phone")
                    .ok()
                    .map(|s| s.to_string());
            }
            if set_doc.contains_key("email") {
                cust.email = set_doc.get_str("email").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("address") {
                cust.address = set_doc.get_str("address").ok().map(|s| s.to_string());
            }
            if let Ok(tags_arr) = set_doc.get_array("tags") {
                cust.tags = tags_arr
                    .iter()
                    .filter_map(|b| b.as_str().map(str::to_string))
                    .collect();
            }
            if set_doc.contains_key("notes") {
                cust.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                cust.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            cust.version += 1;
            cust.updated_at = BsonDateTime::now();

            let tags_json = serde_json::to_string(&cust.tags).unwrap_or_else(|_| "[]".to_string());
            let updated_at_iso = bson_to_iso(&cust.updated_at);

            sqlx::query(
                r#"
                UPDATE customers SET
                    name = ?, contact_person = ?, primary_phone = ?, secondary_phone = ?,
                    email = ?, address = ?, tags = ?, notes = ?, updated_by_device = ?,
                    version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&cust.name)
            .bind(&cust.contact_person)
            .bind(&cust.primary_phone)
            .bind(&cust.secondary_phone)
            .bind(&cust.email)
            .bind(&cust.address)
            .bind(&tags_json)
            .bind(&cust.notes)
            .bind(&cust.updated_by_device)
            .bind(cust.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(cust))
        }
    }
}
