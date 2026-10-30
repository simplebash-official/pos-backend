use super::categories::categories;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;
use crate::modules::inventory::model::CategoryDocument;

pub(crate) async fn insert_category(db: &Db, document: &CategoryDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            categories(db).insert_one(document).await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO categories (
                    key, id, name, icon, color, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.icon)
            .bind(&document.color)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            Ok(())
        }
    }
}
