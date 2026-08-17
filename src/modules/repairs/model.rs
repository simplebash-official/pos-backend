// Mongo document shape for the repairs feature. Kept separate from
// `domain::repairs` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::repairs::Repair,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub ticket_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: String,
    pub device_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub issue_description: String,
    pub status: String,
    pub estimated_cost_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
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
