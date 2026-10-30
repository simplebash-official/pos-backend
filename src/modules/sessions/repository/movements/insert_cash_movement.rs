use super::cash_movements_col::cash_movements_col;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts one movement row (append-only: movements are never edited or deleted).
pub async fn insert_cash_movement(
    unit: impl Into<Unit<'_>>,
    mut doc: CashMovementDocument,
) -> AppResult<CashMovementDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(mongo_db, session) => {
            let res = cash_movements_col(mongo_db)
                .insert_one_in(&doc, session)
                .await?;
            doc.id = res.inserted_id.as_object_id();
            Ok(doc)
        }
        Backend::Sqlite(mut conn) => {
            let id = doc
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at_iso = bson_to_iso(&doc.created_at);
            let updated_at_iso = bson_to_iso(&doc.updated_at);

            sqlx::query(
                r#"
                INSERT INTO cash_movements (
                    key, id, movement_number, movement_type, direction, amount_cents, session_key,
                    terminal_id, category_key, custom_label, payee_key, payee_name,
                    period_start, period_end, reference_no, note,
                    recorded_by_user_id, recorded_by_name_snapshot,
                    approved_by_user_id, approved_by_name_snapshot,
                    approval_reason, reverses_key,
                    recorded_late, original_variance_session_key, receipt_key,
                    version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                )
                "#
            )
            .bind(&doc.key)
            .bind(&id)
            .bind(&doc.movement_number)
            .bind(doc.movement_type.as_str())
            .bind(doc.direction.as_str())
            .bind(doc.amount_cents)
            .bind(&doc.session_key)
            .bind(&doc.terminal_id)
            .bind(&doc.category_key)
            .bind(&doc.custom_label)
            .bind(&doc.payee_key)
            .bind(&doc.payee_name)
            .bind(&doc.period_start)
            .bind(&doc.period_end)
            .bind(&doc.reference_no)
            .bind(&doc.note)
            .bind(&doc.recorded_by_user_id)
            .bind(&doc.recorded_by_name_snapshot)
            .bind(&doc.approved_by_user_id)
            .bind(&doc.approved_by_name_snapshot)
            .bind(&doc.approval_reason)
            .bind(&doc.reverses_key)
            .bind(if doc.recorded_late { 1 } else { 0 })
            .bind(&doc.original_variance_session_key)
            .bind(&doc.receipt_key)
            .bind(doc.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(&mut *conn)
            .await?;

            doc.id = ObjectId::parse_str(&id).ok();
            Ok(doc)
        }
    }
}
