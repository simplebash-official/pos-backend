use crate::domain::print_jobs::{PrintJob, PrintJobFrame, PrintJobItem, PrintJobKind};
use crate::domain::repairs::compute_job_is_overdue;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape representing a print job ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJobDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this print job (e.g. prj_...).
    pub key: String,
    /// Sequential ticket number (e.g. PRN-000001).
    pub ticket_number: String,
    /// Key of the registered customer the job is for.
    pub customer_key: String,
    /// Customer name, read from the customer's record when the job is saved.
    pub customer_name: String,
    /// Customer phone number snapshot.
    pub customer_phone: String,
    /// Type of print job ("mug", "t-shirt", "handbill", "banner", "photo-framing", "custom").
    pub job_type: String,
    /// Number of items to print.
    pub quantity: i32,
    /// Date the job was promised ready (YYYY-MM-DD), if agreed. Absent means none was agreed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promised_ready_at: Option<String>,
    /// Job lifecycle status ("received", "in_repair" (in production), "ready", "delivered",
    /// "cancelled").
    pub status: String,
    /// A staff note about the job; absent for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Ordinary paid work, or a warranty rework of an earlier job.
    pub kind: PrintJobKind,
    /// The job's lines, stored in their API shape (camelCase).
    pub items: Vec<PrintJobItem>,
    /// The frames the customer wants, in their API shape (camelCase); absent when the job lists
    /// none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<PrintJobFrame>>,
    /// Every line's price less its discount, worked out from `items` on every write.
    pub price_cents: i64,
    /// Every line's cost, worked out the same way.
    pub cost_cents: i64,
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
    /// Key of the invoice that billed this job; set together with `billed_invoice_number`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billed_invoice_key: Option<String>,
    /// Number of the invoice that billed this job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billed_invoice_number: Option<String>,
    /// Concurrency version counter for optimistic locking.
    pub version: i64,
    /// Timestamp when print job was created.
    pub created_at: BsonDateTime,
    /// Timestamp when print job was last updated.
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this print job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl PrintJobDocument {
    #[must_use]
    /// Converts the stored document into the type the API returns.
    pub fn into_print_job(self) -> PrintJob {
        let is_overdue = compute_job_is_overdue(&self.status, self.promised_ready_at.as_deref());
        PrintJob {
            id: self
                .id
                .expect("persisted print job document must have an id")
                .to_hex(),
            key: self.key,
            ticket_number: self.ticket_number,
            customer_key: self.customer_key,
            customer_name: self.customer_name,
            customer_phone: self.customer_phone,
            job_type: self.job_type,
            quantity: self.quantity,
            promised_ready_at: self.promised_ready_at,
            is_overdue,
            status: self.status,
            note: self.note,
            kind: self.kind,
            items: self.items,
            // Absent means the job lists no frames.
            frames: self.frames.into_iter().flatten().collect(),
            price_cents: self.price_cents,
            cost_cents: self.cost_cents,
            assigned_employee_id: self.assigned_employee_id,
            assigned_employee_name: self.assigned_employee_name,
            split_type: self.split_type,
            split_value: self.split_value,
            billed_invoice_key: self.billed_invoice_key,
            billed_invoice_number: self.billed_invoice_number,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
