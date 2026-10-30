use super::label_job_item::LabelJobItem;
use super::label_job_outcome::LabelJobOutcome;
use super::label_print_action::LabelPrintAction;
use super::label_print_method::LabelPrintMethod;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A recorded print or download of labels.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelJob {
    /// `lpj_<nanoid>`.
    pub key: String,
    pub template_key: String,
    /// The design's name when it was printed.
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
    /// The person's name when they printed.
    pub printed_by_name: String,
    pub items: Vec<LabelJobItem>,
    pub total_labels: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
