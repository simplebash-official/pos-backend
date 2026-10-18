use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{generate_id_hex, to_bson_datetime},
    },
    core::error::AppResult,
    modules::documents::model::GeneratedDocumentDocument,
};

fn generated_documents(db: &TenantDatabase) -> ScopedCollection<GeneratedDocumentDocument> {
    db.collection("generated_documents")
}

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

fn row_to_document(r: &sqlx::sqlite::SqliteRow) -> GeneratedDocumentDocument {
    let id_str: String = r.get("id");
    let created_str: String = r.get("created_at");
    let updated_str: String = r.get("updated_at");
    GeneratedDocumentDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: r.get("key"),
        entity_key: r.get("entity_key"),
        document_type: r.get("document_type"),
        template_name: r.get("template_name"),
        template_key: r.get("template_key"),
        template_ref: r.get("template_ref"),
        template_title: r.get("template_title"),
        template_version: r.get("template_version"),
        file_path: r.get("file_path"),
        file_size_bytes: r.get("file_size_bytes"),
        created_at: to_bson_datetime(&created_str),
        updated_at: to_bson_datetime(&updated_str),
    }
}

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
