// Storage document shapes for a shop's own label designs and its label print
// history. Kept apart from `domain::labels`, which holds the API-facing types.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::labels::{
    LabelElement, LabelJob, LabelJobItem, LabelJobOutcome, LabelPrintAction, LabelPrintMethod,
    LabelTemplate,
};

/// A label design a shop built (`label_templates`). Tenant-scoped like every
/// other per-shop collection; not a synced resource (shop settings are not).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelTemplateDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

fn default_version() -> i64 {
    1
}

impl LabelTemplateDocument {
    pub fn into_template(self) -> LabelTemplate {
        LabelTemplate {
            key: self.key,
            name: self.name,
            width_mm: self.width_mm,
            height_mm: self.height_mm,
            elements: self.elements,
            built_in: false,
            created_at: Some(self.created_at.to_chrono()),
            updated_at: Some(self.updated_at.to_chrono()),
        }
    }
}

/// One sticker group of a stored print job (snake_case like every stored field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelJobItemDocument {
    pub product_key: String,
    pub product_name: String,
    pub sku: String,
    pub serial_number: Option<String>,
    pub price_cents: Option<i64>,
    pub copies: u32,
}

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
