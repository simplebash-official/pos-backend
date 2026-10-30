use crate::clients::sqlite::to_bson_datetime;
use crate::core::error::{AppError, AppResult};
use crate::domain::cash_categories::{ReportGroup, ReviewStatus};
use crate::domain::sessions::CashMovementType;
use crate::modules::cash_categories::model::CashCategoryDocument;
use mongodb::bson::oid::ObjectId;
use sqlx::Row;

pub(super) fn from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<CashCategoryDocument> {
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
