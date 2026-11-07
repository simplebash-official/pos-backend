//! Dual-engine persistence for password reset tokens.
//!
//! Handles inserting, querying by SHA-256 hash, consumption, and invalidation
//! across both SQLite and MongoDB backends.

use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::core::error::AppResult;
use crate::modules::auth::model::PasswordResetTokenDocument;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

fn password_reset_tokens(db: &TenantDatabase) -> ScopedCollection<PasswordResetTokenDocument> {
    db.platform_collection("password_reset_tokens")
}

fn token_from_sqlite_row(r: &sqlx::sqlite::SqliteRow) -> AppResult<PasswordResetTokenDocument> {
    let id_str: String = r.get("id");
    let expires_at_str: String = r.get("expires_at");
    let used_at_str: Option<String> = r.get("used_at");
    let created_at_str: String = r.get("created_at");

    Ok(PasswordResetTokenDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: r.get("key"),
        user_key: r.get("user_key"),
        token_hash: r.get("token_hash"),
        expires_at: to_bson_datetime(&expires_at_str)?,
        used_at: used_at_str.as_deref().map(to_bson_datetime).transpose()?,
        created_at: to_bson_datetime(&created_at_str)?,
        tenant_id: None,
    })
}

/// Inserts a new password reset token into persistence.
pub(crate) async fn insert_token(
    db: &Db,
    mut document: PasswordResetTokenDocument,
) -> AppResult<PasswordResetTokenDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let result = password_reset_tokens(mongo_db)
                .insert_one(&document)
                .await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let expires_at_str = bson_to_iso(&document.expires_at);
            let used_at_str = document.used_at.as_ref().map(bson_to_iso);
            let created_at_str = bson_to_iso(&document.created_at);

            sqlx::query(
                r#"
                INSERT INTO password_reset_tokens (key, id, user_key, token_hash, expires_at, used_at, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.user_key)
            .bind(&document.token_hash)
            .bind(&expires_at_str)
            .bind(&used_at_str)
            .bind(&created_at_str)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}

/// Finds a password reset token by its SHA-256 hash.
pub(crate) async fn find_token_by_hash(
    db: &Db,
    token_hash: &str,
) -> AppResult<Option<PasswordResetTokenDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let doc = password_reset_tokens(mongo_db)
                .find_one(doc! { "token_hash": token_hash })
                .await?;
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let row = sqlx::query(
                r#"
                SELECT key, id, user_key, token_hash, expires_at, used_at, created_at
                FROM password_reset_tokens
                WHERE token_hash = $1
                "#,
            )
            .bind(token_hash)
            .fetch_optional(pool)
            .await?;

            row.as_ref().map(token_from_sqlite_row).transpose()
        }
    }
}

/// Marks a password reset token as consumed (`used_at = now`).
pub(crate) async fn mark_token_used(db: &Db, key: &str) -> AppResult<()> {
    match db {
        Db::Mongo(mongo_db) => {
            let now = BsonDateTime::now();
            password_reset_tokens(mongo_db)
                .update_one(doc! { "key": key }, doc! { "$set": { "used_at": now } })
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let now_str = crate::clients::sqlite::now_utc_iso();
            sqlx::query("UPDATE password_reset_tokens SET used_at = $1 WHERE key = $2")
                .bind(&now_str)
                .bind(key)
                .execute(pool)
                .await?;
            Ok(())
        }
    }
}

/// Marks all previously active tokens for this user as consumed so they cannot be reused.
pub(crate) async fn invalidate_active_tokens_for_user(db: &Db, user_key: &str) -> AppResult<()> {
    match db {
        Db::Mongo(mongo_db) => {
            let now = BsonDateTime::now();
            password_reset_tokens(mongo_db)
                .update_many(
                    doc! { "user_key": user_key, "used_at": mongodb::bson::Bson::Null },
                    doc! { "$set": { "used_at": now } },
                )
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            let now_str = crate::clients::sqlite::now_utc_iso();
            sqlx::query(
                "UPDATE password_reset_tokens SET used_at = $1 WHERE user_key = $2 AND used_at IS NULL",
            )
            .bind(&now_str)
            .bind(user_key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
