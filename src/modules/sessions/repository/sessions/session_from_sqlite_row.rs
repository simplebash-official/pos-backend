use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::domain::sessions::SessionStatus;
use crate::modules::sessions::model::RegisterSessionDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(crate) fn session_from_sqlite_row(
    row: &sqlx::sqlite::SqliteRow,
) -> AppResult<RegisterSessionDocument> {
    let id_str: String = row.get("id");
    let status_str: String = row.get("status");
    let status = SessionStatus::parse(&status_str).unwrap_or(SessionStatus::Open);

    let opening_denominations_str: Option<String> = row.get("opening_denominations");
    let opening_denominations =
        opening_denominations_str.and_then(|s| serde_json::from_str(&s).ok());

    let closing_denominations_str: Option<String> = row.get("closing_denominations");
    let closing_denominations =
        closing_denominations_str.and_then(|s| serde_json::from_str(&s).ok());

    let opened_at_str: String = row.get("opened_at");
    let closed_at_str: Option<String> = row.get("closed_at");
    let reopened_at_str: Option<String> = row.get("reopened_at");
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let has_late_movements_int: i64 = row.get("has_late_movements");

    Ok(RegisterSessionDocument {
        id: ObjectId::parse_str(&id_str).ok(),
        key: row.get("key"),
        session_number: row.get("session_number"),
        terminal_id: row.get("terminal_id"),
        cashier_id: row.get("cashier_id"),
        cashier_name_snapshot: row.get("cashier_name_snapshot"),
        status,
        opened_at: to_bson_datetime(&opened_at_str),
        opening_float_cents: row.get("opening_float_cents"),
        opening_denominations,
        opening_note: row.get("opening_note"),
        expected_opening_float_cents: row.get("expected_opening_float_cents"),
        opening_discrepancy_cents: row.get("opening_discrepancy_cents"),
        opening_discrepancy_reason: row.get("opening_discrepancy_reason"),
        opening_approved_by_id: row.get("opening_approved_by_id"),
        opening_approved_by_name: row.get("opening_approved_by_name"),
        closed_at: closed_at_str.as_deref().map(to_bson_datetime),
        closing_cashier_id: row.get("closing_cashier_id"),
        closing_cashier_name_snapshot: row.get("closing_cashier_name_snapshot"),
        expected_cash_cents: row.get("expected_cash_cents"),
        counted_cash_cents: row.get("counted_cash_cents"),
        cash_variance_cents: row.get("cash_variance_cents"),
        closing_denominations,
        closing_note: row.get("closing_note"),
        closing_float_cents: row.get("closing_float_cents"),
        cash_drop_to_safe_cents: row.get("cash_drop_to_safe_cents"),
        variance_reason: row.get("variance_reason"),
        variance_approved_by_id: row.get("variance_approved_by_id"),
        variance_approved_by_name: row.get("variance_approved_by_name"),
        reopened_at: reopened_at_str.as_deref().map(to_bson_datetime),
        reopened_by_id: row.get("reopened_by_id"),
        reopened_by_name: row.get("reopened_by_name"),
        reopened_reason: row.get("reopened_reason"),
        total_cash_sales_cents: row.get("total_cash_sales_cents"),
        total_card_sales_cents: row.get("total_card_sales_cents"),
        total_online_sales_cents: row.get("total_online_sales_cents"),
        total_credit_sales_cents: row.get("total_credit_sales_cents"),
        total_cash_refunds_cents: row.get("total_cash_refunds_cents"),
        total_pay_in_cents: row.get("total_pay_in_cents"),
        total_pay_out_cents: row.get("total_pay_out_cents"),
        total_owner_withdrawals_cents: row.get("total_owner_withdrawals_cents"),
        total_cash_drops_cents: row.get("total_cash_drops_cents"),
        has_late_movements: has_late_movements_int != 0,
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
    })
}
