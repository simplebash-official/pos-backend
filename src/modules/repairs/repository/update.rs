use super::find_by_id::find_by_id;
use super::repairs::repairs;
use crate::clients::db::Db;
use crate::clients::sqlite::now_utc_iso;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<RepairDocument>> {
    match db {
        Db::Mongo(db) => Ok(repairs(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_hex = id.to_hex();
            let mut assignments = vec![
                "version = version + 1".to_string(),
                "updated_at = ?".to_string(),
            ];
            let now_iso = now_utc_iso();
            let mut bindings: Vec<Option<String>> = vec![Some(now_iso)];

            for (k, v) in set_doc.iter() {
                if k == "updated_at" {
                    continue;
                }
                match v {
                    mongodb::bson::Bson::Null => {
                        assignments.push(format!("{k} = NULL"));
                    }
                    mongodb::bson::Bson::String(s) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(s.clone()));
                    }
                    mongodb::bson::Bson::Int64(i) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(i.to_string()));
                    }
                    mongodb::bson::Bson::Double(f) => {
                        assignments.push(format!("{k} = ?"));
                        bindings.push(Some(f.to_string()));
                    }
                    _ => {}
                }
            }

            let sql = format!(
                "UPDATE repairs SET {} WHERE id = ? AND deleted_at IS NULL",
                assignments.join(", ")
            );
            let mut query = sqlx::query(&sql);
            for b in bindings {
                query = query.bind(b);
            }
            let res = query.bind(&id_hex).execute(pool).await?;

            if res.rows_affected() == 0 {
                return Ok(None);
            }

            find_by_id(db, id).await
        }
    }
}
