// Mongo document shape for the print-jobs feature. Kept separate from
// `domain::print_jobs` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::print_jobs::PrintJob,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJobDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub ticket_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub job_type: String,
    pub quantity: i32,
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

impl PrintJobDocument {
    pub fn into_print_job(self) -> PrintJob {
        let key = if self.key.is_empty() {
            generate_id(prefixes::PRINT_JOB)
        } else {
            self.key
        };
        PrintJob {
            id: self
                .id
                .expect("persisted print job document must have an id")
                .to_hex(),
            key,
            ticket_number: self.ticket_number,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            job_type: self.job_type,
            quantity: self.quantity,
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
