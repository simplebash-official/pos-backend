// Dual-engine (Mongo / SQLite) access for the `attachments` collection. Append-only.

use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId};
use sqlx::Row;

use crate::{
    clients::{
        db::Db,
        sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime},
        tenant_db::{ScopedCollection, TenantDatabase},
    },
    core::error::{AppError, AppResult},
    domain::attachments::AttachmentOwner,
    modules::attachments::model::AttachmentDocument,
};

fn attachments(db: &TenantDatabase) -> ScopedCollection<AttachmentDocument> {
    db.collection("attachments")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<AttachmentDocument> {
    let kind: String = row.get("owner_kind");
    Ok(AttachmentDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        owner_kind: AttachmentOwner::parse(&kind)
            .ok_or_else(|| AppError::internal(format!("Unknown attachment owner '{kind}' in database")))?,
        owner_key: row.get("owner_key"),
        file_name: row.get("file_name"),
        content_type: row.get("content_type"),
        size_bytes: row.get("size_bytes"),
        data_base64: row.get("data_base64"),
        uploaded_by_user_id: row.get("uploaded_by_user_id"),
        uploaded_by_name_snapshot: row.get("uploaded_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Inserts a photo.
pub(crate) async fn insert(db: &Db, mut document: AttachmentDocument) -> AppResult<AttachmentDocument> {
    match db {
        Db::Mongo(db) => {
            let res = attachments(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document.id.map(|o| o.to_hex()).unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO attachments (
                    key, id, owner_kind, owner_key, file_name, content_type, size_bytes, data_base64,
                    uploaded_by_user_id, uploaded_by_name_snapshot, version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.owner_kind.as_str())
            .bind(&document.owner_key)
            .bind(&document.file_name)
            .bind(&document.content_type)
            .bind(document.size_bytes)
            .bind(&document.data_base64)
            .bind(&document.uploaded_by_user_id)
            .bind(&document.uploaded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .execute(pool)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// One photo by key, image included.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<AttachmentDocument>> {
    match db {
        Db::Mongo(db) => Ok(attachments(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM attachments WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// The photos of one record, oldest first.
pub(crate) async fn list_for_owner(
    db: &Db,
    owner_kind: AttachmentOwner,
    owner_key: &str,
) -> AppResult<Vec<AttachmentDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = attachments(db)
                .find(doc! { "owner_kind": owner_kind.as_str(), "owner_key": owner_key })
                .sort(doc! { "created_at": 1 })
                .await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM attachments WHERE owner_kind = ? AND owner_key = ? ORDER BY created_at ASC",
            )
            .bind(owner_kind.as_str())
            .bind(owner_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(from_row).collect()
        }
    }
}
