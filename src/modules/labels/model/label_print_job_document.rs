use super::label_job_item_document::LabelJobItemDocument;
use crate::domain::labels::{
    LabelJob, LabelJobItem, LabelJobOutcome, LabelPrintAction, LabelPrintMethod,
};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A print or download of labels (`label_print_jobs`, key `lpj_…`). Tenant-scoped
/// and, like `label_templates`, not synced: each install keeps its own history.
/// Names (design, person, products) are snapshots taken when it was printed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelPrintJobDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub template_key: String,
    pub template_name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub action: LabelPrintAction,
    pub method: Option<LabelPrintMethod>,
    pub printer_name: Option<String>,
    pub dpi: Option<u32>,
    pub outcome: LabelJobOutcome,
    pub error: Option<String>,
    pub printed_by_user_id: String,
    pub printed_by_name: String,
    pub items: Vec<LabelJobItemDocument>,
    pub total_labels: u32,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl LabelPrintJobDocument {
    #[must_use]
    pub fn into_job(self) -> LabelJob {
        LabelJob {
            key: self.key,
            template_key: self.template_key,
            template_name: self.template_name,
            width_mm: self.width_mm,
            height_mm: self.height_mm,
            action: self.action,
            method: self.method,
            printer_name: self.printer_name,
            dpi: self.dpi,
            outcome: self.outcome,
            error: self.error,
            printed_by_user_id: self.printed_by_user_id,
            printed_by_name: self.printed_by_name,
            items: self
                .items
                .into_iter()
                .map(|item| LabelJobItem {
                    product_key: item.product_key,
                    product_name: item.product_name,
                    sku: item.sku,
                    serial_number: item.serial_number,
                    price_cents: item.price_cents,
                    copies: item.copies,
                })
                .collect(),
            total_labels: self.total_labels,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
        }
    }
}
