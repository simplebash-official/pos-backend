use super::sessions_col::sessions_col;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;

use crate::modules::sessions::model::RegisterSessionDocument;

use mongodb::bson::oid::ObjectId;

pub async fn insert_session(
    db: &Db,
    mut doc: RegisterSessionDocument,
) -> AppResult<RegisterSessionDocument> {
    match db {
        Db::Mongo(mongo_db) => {
            let res = sessions_col(mongo_db).insert_one(&doc).await?;
            doc.id = res.inserted_id.as_object_id();
            Ok(doc)
        }
        Db::Sqlite(pool) => {
            let id = doc
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let opened_at_iso = bson_to_iso(&doc.opened_at);
            let closed_at_iso = doc.closed_at.as_ref().map(bson_to_iso);
            let reopened_at_iso = doc.reopened_at.as_ref().map(bson_to_iso);
            let created_at_iso = bson_to_iso(&doc.created_at);
            let updated_at_iso = bson_to_iso(&doc.updated_at);
            let opening_denom_json = doc
                .opening_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            let closing_denom_json = doc
                .closing_denominations
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());

            sqlx::query(
                r#"
                INSERT INTO register_sessions (
                    key, id, session_number, terminal_id, cashier_id, cashier_name_snapshot,
                    status, opened_at, opening_float_cents, opening_denominations, opening_note,
                    expected_opening_float_cents, opening_discrepancy_cents, opening_discrepancy_reason,
                    opening_approved_by_id, opening_approved_by_name, closed_at,
                    closing_cashier_id, closing_cashier_name_snapshot,
                    expected_cash_cents, counted_cash_cents, cash_variance_cents,
                    closing_denominations, closing_note, closing_float_cents,
                    cash_drop_to_safe_cents, variance_reason, variance_approved_by_id,
                    variance_approved_by_name, reopened_at, reopened_by_id, reopened_by_name,
                    reopened_reason, total_cash_sales_cents, total_card_sales_cents,
                    total_online_sales_cents, total_credit_sales_cents, total_cash_refunds_cents,
                    total_pay_in_cents, total_pay_out_cents, total_owner_withdrawals_cents,
                    total_cash_drops_cents, has_late_movements, version, created_at, updated_at
                ) VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?
                )
                "#
            )
            .bind(&doc.key)
            .bind(&id)
            .bind(&doc.session_number)
            .bind(&doc.terminal_id)
            .bind(&doc.cashier_id)
            .bind(&doc.cashier_name_snapshot)
            .bind(doc.status.as_str())
            .bind(&opened_at_iso)
            .bind(doc.opening_float_cents)
            .bind(&opening_denom_json)
            .bind(&doc.opening_note)
            .bind(doc.expected_opening_float_cents)
            .bind(doc.opening_discrepancy_cents)
            .bind(&doc.opening_discrepancy_reason)
            .bind(&doc.opening_approved_by_id)
            .bind(&doc.opening_approved_by_name)
            .bind(&closed_at_iso)
            .bind(&doc.closing_cashier_id)
            .bind(&doc.closing_cashier_name_snapshot)
            .bind(doc.expected_cash_cents)
            .bind(doc.counted_cash_cents)
            .bind(doc.cash_variance_cents)
            .bind(&closing_denom_json)
            .bind(&doc.closing_note)
            .bind(doc.closing_float_cents)
            .bind(doc.cash_drop_to_safe_cents)
            .bind(&doc.variance_reason)
            .bind(&doc.variance_approved_by_id)
            .bind(&doc.variance_approved_by_name)
            .bind(&reopened_at_iso)
            .bind(&doc.reopened_by_id)
            .bind(&doc.reopened_by_name)
            .bind(&doc.reopened_reason)
            .bind(doc.total_cash_sales_cents)
            .bind(doc.total_card_sales_cents)
            .bind(doc.total_online_sales_cents)
            .bind(doc.total_credit_sales_cents)
            .bind(doc.total_cash_refunds_cents)
            .bind(doc.total_pay_in_cents)
            .bind(doc.total_pay_out_cents)
            .bind(doc.total_owner_withdrawals_cents)
            .bind(doc.total_cash_drops_cents)
            .bind(if doc.has_late_movements { 1 } else { 0 })
            .bind(doc.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            doc.id = ObjectId::parse_str(&id).ok();
            Ok(doc)
        }
    }
}
