use super::generated_documents::generated_documents;
use super::row_to_document::row_to_document;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::documents::model::GeneratedDocumentDocument;
use mongodb::bson::doc;

/// The newest stored document of `entity_key` for a print variant, whatever
/// design rendered it: the variant's own key (`legacy_key`, the built-in
/// default design) or `legacy_key--<template name>` (any other design). Backs
/// "print the original".
pub(crate) async fn find_latest_by_entity_and_variant(
    db: &Db,
    entity_key: &str,
    legacy_key: &str,
) -> AppResult<Option<GeneratedDocumentDocument>> {
    let prefixed = format!("{legacy_key}--");
    match db {
        Db::Mongo(db) => Ok(generated_documents(db)
            .find_one(doc! {
                "entity_key": entity_key,
                "$or": [
                    { "document_type": legacy_key },
                    { "document_type": { "$regex": format!("^{}", regex_escape(&prefixed)) } },
                ],
            })
            .sort(doc! { "created_at": -1 })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"
                SELECT key, id, entity_key, document_type, template_name, template_key, template_ref, template_title, template_version, file_path, file_size_bytes, created_at, updated_at
                FROM generated_documents
                WHERE entity_key = $1
                  AND (document_type = $2 OR substr(document_type, 1, length($3)) = $3)
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(entity_key)
            .bind(legacy_key)
            .bind(&prefixed)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(|r| row_to_document(&r)))
        }
    }
}

/// Escapes regex metacharacters (the keys are `[a-z0-9-]`, but a regex built
/// from data is never left unescaped).
fn regex_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                vec![c]
            } else {
                vec!['\\', c]
            }
        })
        .collect()
}
