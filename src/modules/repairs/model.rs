// Mongo document shape for the repairs feature. Kept separate from
// `domain::repairs` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::repairs::{Repair, compute_job_is_overdue},
};

/// Mongo document shape representing a repair ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this repair ticket (e.g. rep_...).
    #[serde(default)]
    pub key: String,
    /// Sequential ticket number (e.g. REP-000001).
    pub ticket_number: String,
    /// Key of registered customer if linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer name snapshot or walk-in name.
    pub customer_name: String,
    /// Customer phone number.
    pub customer_phone: String,
    /// Make and model of the device being repaired (e.g. iPhone 13).
    pub device_model: String,
    /// Device serial number or IMEI, if provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Problem description or repair diagnosis notes.
    pub issue_description: String,
    /// Date the job was promised ready for the customer (YYYY-MM-DD), if a
    /// hand-back date was agreed. `#[serde(default)]` keeps older stored
    /// documents (written before this field existed) deserializing cleanly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promised_ready_at: Option<String>,
    /// Lifecycle status ("received", "in_progress", "completed", "delivered", "cancelled").
    pub status: String,
    /// Estimated or final charge to customer in cents. Absent until quoted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_cents: Option<i64>,
    /// Direct cost of replacement parts/materials used in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
    /// User ID of the technician assigned to the repair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    /// Display name of technician assigned to the repair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Commission split calculation method ("percentage" or "fixed").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    /// Commission rate or fixed value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when repair ticket was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when repair ticket was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this repair record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl RepairDocument {
    pub fn into_repair(self) -> Repair {
        let key = if self.key.is_empty() {
            generate_id(prefixes::REPAIR)
        } else {
            self.key
        };
        let is_overdue = compute_job_is_overdue(&self.status, self.promised_ready_at.as_deref());
        Repair {
            id: self
                .id
                .expect("persisted repair document must have an id")
                .to_hex(),
            key,
            ticket_number: self.ticket_number,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            device_model: self.device_model,
            serial_number: self.serial_number,
            issue_description: self.issue_description,
            promised_ready_at: self.promised_ready_at,
            is_overdue,
            status: self.status,
            estimated_cost_cents: self.estimated_cost_cents,
            material_cost_cents: self.material_cost_cents,
            assigned_employee_id: self.assigned_employee_id,
            assigned_employee_name: self.assigned_employee_name,
            split_type: self.split_type,
            split_value: self.split_value,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
