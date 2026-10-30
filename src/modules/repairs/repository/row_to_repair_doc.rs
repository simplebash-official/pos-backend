use crate::clients::sqlite::to_bson_datetime;

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::oid::ObjectId;

use sqlx::Row;

pub(super) fn row_to_repair_doc(row: &sqlx::sqlite::SqliteRow) -> AppResult<RepairDocument> {
    let id_str: String = row.get("id");
    let id = ObjectId::parse_str(&id_str).ok();
    let created_at_str: String = row.get("created_at");
    let updated_at_str: String = row.get("updated_at");
    let deleted_at_str: Option<String> = row.get("deleted_at");

    Ok(RepairDocument {
        id,
        key: row.get("key"),
        ticket_number: row.get("ticket_number"),
        customer_key: row.get("customer_key"),
        customer_name: row.get("customer_name"),
        customer_phone: row.get("customer_phone"),
        device_model: row.get("device_model"),
        serial_number: row.get("serial_number"),
        issue_description: row.get("issue_description"),
        promised_ready_at: row.get("promised_ready_at"),
        status: row.get("status"),
        estimated_cost_cents: row.get("estimated_cost_cents"),
        material_cost_cents: row.get("material_cost_cents"),
        assigned_employee_id: row.get("assigned_employee_id"),
        assigned_employee_name: row.get("assigned_employee_name"),
        split_type: row.get("split_type"),
        split_value: row.get("split_value"),
        version: row.get("version"),
        created_at: to_bson_datetime(&created_at_str),
        updated_at: to_bson_datetime(&updated_at_str),
        deleted_at: deleted_at_str.map(|s| to_bson_datetime(&s)),
        updated_by_device: row.get("updated_by_device"),
    })
}
