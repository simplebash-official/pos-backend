use super::categories::categories;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::cash_categories::model::CashCategoryDocument;
use mongodb::bson::oid::ObjectId;

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
