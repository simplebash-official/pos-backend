use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex, to_bson_datetime};
use crate::clients::tenant_db::{ScopedCollection, TenantDatabase};
use crate::clients::uow::{Backend, Unit};
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::{Tenant, with_tenant};
use crate::domain::notifications::{
    NotificationCategory, NotificationSeverity, NotificationSource,
};
use crate::modules::notifications::model::{
    AudienceKind, NotificationDocument, NotificationReadDocument,
};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::doc;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

pub(super) const NOTIFICATIONS: &str = "notifications";
pub(super) const READS: &str = "notification_reads";

pub(super) fn notifications(db: &TenantDatabase) -> ScopedCollection<NotificationDocument> {
    db.collection(NOTIFICATIONS)
}

pub(super) fn reads(db: &TenantDatabase) -> ScopedCollection<NotificationReadDocument> {
    db.collection(READS)
}

/// One stored notification by key, inside the unit's transaction when there is one.
pub(crate) async fn find_by_key(
    unit: impl Into<Unit<'_>>,
    key: &str,
) -> AppResult<Option<NotificationDocument>> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let collection = notifications(db);
            let find = collection.find_one(doc! { "key": key });
            Ok(match session {
                Some(session) => find.session(session).await?,
                None => find.await?,
            })
        }
        Backend::Sqlite(mut conn) => {
            let row = sqlx::query("SELECT * FROM notifications WHERE key = ?")
                .bind(key)
                .fetch_optional(&mut *conn)
                .await?;
            row.as_ref().map(notification_from_row).transpose()
        }
    }
}

/// Inserts a new notification. The caller has checked that its key is free.
pub(crate) async fn insert(
    unit: impl Into<Unit<'_>>,
    document: &NotificationDocument,
) -> AppResult<()> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            notifications(db).insert_one_in(document, session).await?;
            Ok(())
        }
        Backend::Sqlite(mut conn) => {
            sqlx::query(
                "INSERT INTO notifications (key, id, category, severity, title, body, link_path,
                     link_label, audience_type, audience_value, source, dedupe_key,
                     raised_by_user_key, version, created_at, updated_at, updated_by_device)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(generate_id_hex())
            .bind(document.category.as_str())
            .bind(document.severity.as_str())
            .bind(&document.title)
            .bind(&document.body)
            .bind(&document.link_path)
            .bind(&document.link_label)
            .bind(document.audience_type.as_str())
            .bind(&document.audience_value)
            .bind(document.source.as_str())
            .bind(&document.dedupe_key)
            .bind(&document.raised_by_user_key)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.updated_by_device)
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
    }
}

/// Inserts each read mark whose key is not stored yet; returns how many were new.
pub(crate) async fn insert_reads_if_absent(
    db: &Db,
    documents: &[NotificationReadDocument],
) -> AppResult<u64> {
    if documents.is_empty() {
        return Ok(0);
    }
    match db {
        Db::Mongo(db) => {
            let keys: Vec<&str> = documents.iter().map(|d| d.key.as_str()).collect();
            let existing: Vec<String> = reads(db)
                .distinct("key", doc! { "key": { "$in": &keys } })
                .await?
                .into_iter()
                .filter_map(|b| b.as_str().map(str::to_string))
                .collect();
            let fresh: Vec<&NotificationReadDocument> = documents
                .iter()
                .filter(|d| !existing.contains(&d.key))
                .collect();
            for document in &fresh {
                // A concurrent mark of the same read (another tab) lands on the unique key:
                // the read is recorded either way, so that one race is not an error.
                if let Err(err) = reads(db).insert_one(*document).await
                    && !is_duplicate_key(&err)
                {
                    return Err(err.into());
                }
            }
            Ok(fresh.len() as u64)
        }
        Db::Sqlite(pool) => {
            let mut tx = pool.begin().await?;
            let mut inserted = 0;
            for document in documents {
                let result = sqlx::query(
                    "INSERT INTO notification_reads (key, id, notification_key, user_key, version,
                         created_at, updated_at, updated_by_device)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(key) DO NOTHING",
                )
                .bind(&document.key)
                .bind(generate_id_hex())
                .bind(&document.notification_key)
                .bind(&document.user_key)
                .bind(document.version)
                .bind(bson_to_iso(&document.created_at))
                .bind(bson_to_iso(&document.updated_at))
                .bind(&document.updated_by_device)
                .execute(&mut *tx)
                .await?;
                inserted += result.rows_affected();
            }
            tx.commit().await?;
            Ok(inserted)
        }
    }
}

/// Deletes every notification raised before `cutoff`, and every read mark of them, in every
/// shop. Returns how many notifications were removed.
pub(crate) async fn purge_older_than(db: &Db, cutoff: BsonDateTime) -> AppResult<u64> {
    match db {
        // The retention rule is the same for every shop, so the purge runs unscoped.
        Db::Mongo(db) => {
            with_tenant(Tenant::Global, async {
                let old: Vec<String> = notifications(db)
                    .distinct("key", doc! { "created_at": { "$lt": cutoff } })
                    .await?
                    .into_iter()
                    .filter_map(|b| b.as_str().map(str::to_string))
                    .collect();
                if old.is_empty() {
                    return Ok(0);
                }
                reads(db)
                    .delete_many(doc! { "notification_key": { "$in": &old } })
                    .await?;
                let removed = notifications(db)
                    .delete_many(doc! { "key": { "$in": &old } })
                    .await?;
                Ok(removed.deleted_count)
            })
            .await
        }
        Db::Sqlite(pool) => {
            let cutoff = bson_to_iso(&cutoff);
            let mut tx = pool.begin().await?;
            sqlx::query(
                "DELETE FROM notification_reads WHERE notification_key IN
                     (SELECT key FROM notifications WHERE julianday(created_at) < julianday(?))",
            )
            .bind(&cutoff)
            .execute(&mut *tx)
            .await?;
            let removed =
                sqlx::query("DELETE FROM notifications WHERE julianday(created_at) < julianday(?)")
                    .bind(&cutoff)
                    .execute(&mut *tx)
                    .await?;
            tx.commit().await?;
            Ok(removed.rows_affected())
        }
    }
}

fn is_duplicate_key(err: &mongodb::error::Error) -> bool {
    use mongodb::error::{ErrorKind, WriteFailure};
    matches!(
        err.kind.as_ref(),
        ErrorKind::Write(WriteFailure::WriteError(e)) if e.code == 11000
    )
}

pub(super) fn notification_from_row(row: &SqliteRow) -> AppResult<NotificationDocument> {
    let key: String = row.get("key");
    let unknown = |column: &str, value: &str| {
        AppError::internal(format!(
            "Notification {key} has unknown {column} '{value}' in database"
        ))
    };
    let category: String = row.get("category");
    let severity: String = row.get("severity");
    let audience: String = row.get("audience_type");
    let source: String = row.get("source");
    Ok(NotificationDocument {
        // `_id` is MongoDB's own; nothing reads it on SQLite.
        id: None,
        category: NotificationCategory::parse(&category)
            .ok_or_else(|| unknown("category", &category))?,
        severity: NotificationSeverity::parse(&severity)
            .ok_or_else(|| unknown("severity", &severity))?,
        title: row.get("title"),
        body: row.get("body"),
        link_path: row.get("link_path"),
        link_label: row.get("link_label"),
        audience_type: AudienceKind::parse(&audience)
            .ok_or_else(|| unknown("audience", &audience))?,
        audience_value: row.get("audience_value"),
        source: NotificationSource::parse(&source).ok_or_else(|| unknown("source", &source))?,
        dedupe_key: row.get("dedupe_key"),
        raised_by_user_key: row.get("raised_by_user_key"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at"))?,
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at"))?,
        updated_by_device: row.get("updated_by_device"),
        key,
    })
}
