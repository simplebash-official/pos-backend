// Dual-engine (Mongo / SQLite) access for the `cash_categories` collection.
// Returns documents straight from the driver; business rules live in `service`.

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
    domain::{
        cash_categories::{ReportGroup, ReviewStatus},
        sessions::CashMovementType,
    },
    modules::cash_categories::model::CashCategoryDocument,
};

fn categories(db: &TenantDatabase) -> ScopedCollection<CashCategoryDocument> {
    db.collection("cash_categories")
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashCategoryDocument> {
    let type_str: String = row.get("movement_type");
    let group_str: String = row.get("report_group");
    let review_str: String = row.get("review_status");
    Ok(CashCategoryDocument {
        id: ObjectId::parse_str(row.get::<String, _>("id")).ok(),
        key: row.get("key"),
        movement_type: CashMovementType::parse(&type_str).ok_or_else(|| {
            AppError::internal(format!(
                "Unknown movement type '{type_str}' on a cash category"
            ))
        })?,
        name: row.get("name"),
        name_si: row.get("name_si"),
        normalized_name: row.get("normalized_name"),
        report_group: ReportGroup::parse(&group_str).ok_or_else(|| {
            AppError::internal(format!(
                "Unknown report group '{group_str}' on a cash category"
            ))
        })?,
        is_system: row.get::<i64, _>("is_system") != 0,
        review_status: ReviewStatus::parse(&review_str).ok_or_else(|| {
            AppError::internal(format!(
                "Unknown review status '{review_str}' on a cash category"
            ))
        })?,
        pinned: row.get::<i64, _>("pinned") != 0,
        sort_order: row.get("sort_order"),
        active: row.get::<i64, _>("active") != 0,
        requires_receipt: row.get::<i64, _>("requires_receipt") != 0,
        requires_approval_over_cents: row.get("requires_approval_over_cents"),
        merged_into_key: row.get("merged_into_key"),
        created_by_name_snapshot: row.get("created_by_name_snapshot"),
        version: row.get("version"),
        created_at: to_bson_datetime(&row.get::<String, _>("created_at")),
        updated_at: to_bson_datetime(&row.get::<String, _>("updated_at")),
        updated_by_device: row.get("updated_by_device"),
    })
}

/// Every category of the shop, hidden and merged ones included; the service filters.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<CashCategoryDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = categories(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM cash_categories")
                .fetch_all(pool)
                .await?;
            rows.iter().map(from_row).collect()
        }
    }
}

/// One category by its key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<CashCategoryDocument>> {
    match db {
        Db::Mongo(db) => Ok(categories(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM cash_categories WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}

/// Inserts a new category.
pub(crate) async fn insert(
    db: &Db,
    mut document: CashCategoryDocument,
) -> AppResult<CashCategoryDocument> {
    match db {
        Db::Mongo(db) => {
            let res = categories(db).insert_one(&document).await?;
            document.id = res.inserted_id.as_object_id();
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|o| o.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO cash_categories (
                    key, id, movement_type, name, name_si, normalized_name, report_group, is_system,
                    review_status, pinned, sort_order, active, requires_receipt,
                    requires_approval_over_cents, merged_into_key, created_by_name_snapshot,
                    version, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(document.movement_type.as_str())
            .bind(&document.name)
            .bind(&document.name_si)
            .bind(&document.normalized_name)
            .bind(document.report_group.as_str())
            .bind(i64::from(document.is_system))
            .bind(document.review_status.as_str())
            .bind(i64::from(document.pinned))
            .bind(document.sort_order)
            .bind(i64::from(document.active))
            .bind(i64::from(document.requires_receipt))
            .bind(document.requires_approval_over_cents)
            .bind(&document.merged_into_key)
            .bind(&document.created_by_name_snapshot)
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

/// Writes every editable field of an existing category back (the service has already bumped `version`/`updated_at`).
pub(crate) async fn save(db: &Db, document: &CashCategoryDocument) -> AppResult<()> {
    match db {
        Db::Mongo(db) => {
            categories(db)
                .replace_one(doc! { "key": &document.key }, document)
                .await?;
            Ok(())
        }
        Db::Sqlite(pool) => {
            sqlx::query(
                "UPDATE cash_categories SET name = ?, name_si = ?, normalized_name = ?, report_group = ?,
                    review_status = ?, pinned = ?, sort_order = ?, active = ?, requires_receipt = ?,
                    requires_approval_over_cents = ?, merged_into_key = ?, version = ?, updated_at = ?
                 WHERE key = ?",
            )
            .bind(&document.name)
            .bind(&document.name_si)
            .bind(&document.normalized_name)
            .bind(document.report_group.as_str())
            .bind(document.review_status.as_str())
            .bind(i64::from(document.pinned))
            .bind(document.sort_order)
            .bind(i64::from(document.active))
            .bind(i64::from(document.requires_receipt))
            .bind(document.requires_approval_over_cents)
            .bind(&document.merged_into_key)
            .bind(document.version)
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.key)
            .execute(pool)
            .await?;
            Ok(())
        }
    }
}
