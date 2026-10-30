use super::generated_documents::generated_documents;
use crate::clients::db::Db;
use crate::clients::sqlite::generate_id_hex;

use crate::core::error::AppResult;
use crate::modules::documents::model::GeneratedDocumentDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert(
    db: &Db,
    mut document: GeneratedDocumentDocument,
) -> AppResult<GeneratedDocumentDocument> {
    match db {
        Db::Mongo(db) => {
            let result = generated_documents(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id_str = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_str = document.created_at.to_chrono().to_rfc3339();
            let updated_str = document.updated_at.to_chrono().to_rfc3339();

            sqlx::query(
                r#"
                INSERT INTO generated_documents (key, id, entity_key, document_type, template_name, template_key, template_ref, template_title, template_version, file_path, file_size_bytes, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(&document.key)
            .bind(&id_str)
            .bind(&document.entity_key)
            .bind(&document.document_type)
            .bind(&document.template_name)
            .bind(&document.template_key)
            .bind(&document.template_ref)
            .bind(&document.template_title)
            .bind(document.template_version)
            .bind(&document.file_path)
            .bind(document.file_size_bytes)
            .bind(&created_str)
            .bind(&updated_str)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id_str).ok();
            Ok(document)
        }
    }
}
