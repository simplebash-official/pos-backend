use super::generated_documents::generated_documents;
use super::row_to_document::row_to_document;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::documents::model::GeneratedDocumentDocument;
use mongodb::bson::doc;

pub(crate) async fn find_latest_by_entity_and_type(
    db: &Db,
    entity_key: &str,
    document_type: &str,
) -> AppResult<Option<GeneratedDocumentDocument>> {
    match db {
        Db::Mongo(db) => Ok(generated_documents(db)
            .find_one(doc! { "entity_key": entity_key, "document_type": document_type })
            .sort(doc! { "created_at": -1 })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"
                SELECT key, id, entity_key, document_type, template_name, template_key, template_ref, template_title, template_version, file_path, file_size_bytes, created_at, updated_at
                FROM generated_documents
                WHERE entity_key = $1 AND document_type = $2
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(entity_key)
            .bind(document_type)
            .fetch_optional(pool)
            .await?;

            Ok(row.map(|r| row_to_document(&r)))
        }
    }
}
