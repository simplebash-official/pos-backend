use super::find_supplier_by_id::find_supplier_by_id;
use super::suppliers::suppliers;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;

use crate::core::error::AppResult;
use crate::domain::suppliers::ContactPerson;
use crate::modules::suppliers::model::SupplierDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_supplier(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<SupplierDocument>> {
    match db {
        Db::Mongo(db) => Ok(suppliers(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_supplier_by_id(db, id).await?;
            let Some(mut sup) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                sup.name = name.to_string();
            }
            if let Ok(contact) = set_doc.get_str("contact_person") {
                sup.contact_person = contact.to_string();
            }
            if let Ok(phone) = set_doc.get_str("primary_phone") {
                sup.primary_phone = phone.to_string();
            }
            if set_doc.contains_key("secondary_phone") {
                sup.secondary_phone = set_doc
                    .get_str("secondary_phone")
                    .ok()
                    .map(|s| s.to_string());
            }
            if set_doc.contains_key("email") {
                sup.email = set_doc.get_str("email").ok().map(|s| s.to_string());
            }
            if let Ok(addr) = set_doc.get_str("address") {
                sup.address = addr.to_string();
            }
            if let Ok(cats_arr) = set_doc.get_array("supplied_categories") {
                sup.supplied_categories = cats_arr
                    .iter()
                    .filter_map(|b| b.as_str().map(str::to_string))
                    .collect();
            }
            if let Ok(cp_arr) = set_doc.get_array("contact_persons") {
                sup.contact_persons = cp_arr
                    .iter()
                    .filter_map(|b| {
                        mongodb::bson::deserialize_from_bson::<ContactPerson>(b.clone()).ok()
                    })
                    .collect();
            }
            if set_doc.contains_key("notes") {
                sup.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                sup.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            sup.version += 1;
            sup.updated_at = BsonDateTime::now();

            let categories_json = serde_json::to_string(&sup.supplied_categories)
                .unwrap_or_else(|_| "[]".to_string());
            let contact_persons_json =
                serde_json::to_string(&sup.contact_persons).unwrap_or_else(|_| "[]".to_string());
            let updated_at_iso = bson_to_iso(&sup.updated_at);

            sqlx::query(
                r#"
                UPDATE suppliers SET
                    name = ?, contact_person = ?, primary_phone = ?, secondary_phone = ?,
                    contact_persons = ?, email = ?, address = ?, supplied_categories = ?, notes = ?,
                    updated_by_device = ?, version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&sup.name)
            .bind(&sup.contact_person)
            .bind(&sup.primary_phone)
            .bind(&sup.secondary_phone)
            .bind(&contact_persons_json)
            .bind(&sup.email)
            .bind(&sup.address)
            .bind(&categories_json)
            .bind(&sup.notes)
            .bind(&sup.updated_by_device)
            .bind(sup.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(sup))
        }
    }
}
