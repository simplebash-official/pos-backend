// Mongo document shape for the print-jobs feature. Kept separate from
// `domain::print_jobs` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::print_jobs::PrintJob,
};

/// Mongo document shape representing a print job ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJobDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this print job (e.g. prj_...).
    #[serde(default)]
    pub key: String,
    /// Sequential ticket number (e.g. PRN-000001).
    pub ticket_number: String,
    /// Key of registered customer if linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer name snapshot or walk-in name.
    pub customer_name: String,
    /// Customer phone number snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// Type of print job ("mug", "t-shirt", "handbill", "banner", "custom").
    pub job_type: String,
    /// Number of items to print.
    pub quantity: i32,
    /// Job lifecycle status ("received", "in_progress", "completed", "delivered", "cancelled").
    pub status: String,
    /// Estimated charge to customer in cents.
    pub estimated_cost_cents: i64,
    /// Internal raw material cost in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_cost_cents: Option<i64>,
    /// User ID of employee assigned to perform this job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_id: Option<String>,
    /// Name snapshot of employee assigned to this job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Commission split calculation type ("percentage" or "fixed").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_type: Option<String>,
    /// Commission split rate or fixed value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_value: Option<f64>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when print job was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when print job was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this print job.
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
