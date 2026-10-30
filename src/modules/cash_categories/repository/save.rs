use super::categories::categories;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::cash_categories::model::CashCategoryDocument;
use mongodb::bson::doc;

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
