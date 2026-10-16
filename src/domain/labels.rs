// Pure business types for label printing: a shop-designed label layout and the
// requests that print it. No I/O, no Mongo/Axum types beyond serde/utoipa
// derives. The layout is plain data (absolutely placed elements, in
// millimetres); document-server's "Product Sticker Label" template draws it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::inventory::{PaginationMeta, Product};

/// What a text element prints. `Code` is the value the barcode/QR encodes: the
/// product's barcode on a product label, the serial number on a serial label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TextSource {
    ProductName,
    Price,
    Sku,
    Code,
    Category,
    ShopName,
    CustomText,
}

/// Horizontal alignment of a text element inside its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// One thing drawn on a label. Positions and sizes are millimetres from the
/// label's top-left corner. Every field is required (no implicit defaults);
/// `custom_text` is `null` for every source except `customText`. This is also
/// the exact shape the document-server template's schema accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LabelElement {
    #[serde(rename_all = "camelCase")]
    Text {
        id: String,
        source: TextSource,
        custom_text: Option<String>,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        font_size_pt: f64,
        bold: bool,
        align: TextAlign,
    },
    /// A Code128 barcode of the label's code; `show_value` prints it underneath.
    #[serde(rename_all = "camelCase")]
    Barcode {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
        show_value: bool,
        font_size_pt: f64,
    },
    /// A QR code of the label's code, drawn as a square in the box.
    #[serde(rename_all = "camelCase")]
    Qr {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
    /// A solid black rectangle, used as a rule or divider.
    #[serde(rename_all = "camelCase")]
    Line {
        id: String,
        x_mm: f64,
        y_mm: f64,
        width_mm: f64,
        height_mm: f64,
    },
}

impl LabelElement {
    /// Geometry shared by every element kind: `(id, x, y, width, height)`.
    pub fn frame(&self) -> (&str, f64, f64, f64, f64) {
        match self {
            Self::Text {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                ..
            }
            | Self::Barcode {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
                ..
            }
            | Self::Qr {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
            }
            | Self::Line {
                id,
                x_mm,
                y_mm,
                width_mm,
                height_mm,
            } => (id, *x_mm, *y_mm, *width_mm, *height_mm),
        }
    }
}

/// A label design as the API returns it: a ready-made starter (`built_in`,
/// read-only, no timestamps) or one the shop built.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplate {
    /// `ltpl_<nanoid>` for a shop's template, `starter_<name>` for a built-in one.
    pub key: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
    pub built_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/// Body of `POST /labels/templates` and `PUT /labels/templates/{key}`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplateInput {
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub elements: Vec<LabelElement>,
}

/// `GET /labels/templates` result: starters first, then the shop's own by name.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelTemplatesResponse {
    pub templates: Vec<LabelTemplate>,
}

/// One thing to print. A product label prints the product's barcode; a serial
/// label prints that unit's serial number at its own batch's price.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LabelItem {
    #[serde(rename_all = "camelCase")]
    Product {
        product_key: String,
        copies: u32,
        /// One of the product's current selling prices; required when the
        /// chosen template has a price element, `null` otherwise.
        price_cents: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Serial { serial_number: String, copies: u32 },
}

/// Body of `POST /labels/render`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenderLabelsRequest {
    pub template_key: String,
    pub items: Vec<LabelItem>,
}

/// Example values a design is previewed with before it is saved.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelSample {
    pub name: String,
    pub sku: String,
    pub category: String,
    pub price_cents: i64,
    pub code: String,
}

/// Body of `POST /labels/preview`: an unsaved design plus the sample to draw.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLabelsRequest {
    pub template: LabelTemplateInput,
    pub sample: LabelSample,
}

/// What the person did with the stickers: sent them to a printer or saved the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelPrintAction {
    Print,
    Download,
}

/// How a print left the computer: the system print window, or straight to a
/// named printer (desktop only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelPrintMethod {
    System,
    Direct,
}

/// Where a print job stands. `Rendered` until the computer reports back; a
/// download is `Sent` from the start (the file was handed over). A `Failed`
/// job does not count as labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelJobOutcome {
    Rendered,
    Sent,
    Failed,
}

/// Body of `POST /labels/jobs`: what to print plus how it leaves the computer.
/// `method` and `dpi` are required for a print and must be absent for a
/// download; `printer_name` is required exactly when `method` is `direct`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelJobRequest {
    pub template_key: String,
    pub items: Vec<LabelItem>,
    pub action: LabelPrintAction,
    pub method: Option<LabelPrintMethod>,
    pub printer_name: Option<String>,
    pub dpi: Option<u32>,
}

/// Body of `PATCH /labels/jobs/{key}`: what the printer step did. `outcome`
/// must be `sent` or `failed`; `error` is the reason, only with `failed`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLabelJobRequest {
    pub outcome: LabelJobOutcome,
    pub error: Option<String>,
}

/// One sticker group of a print job, as it was printed (a snapshot: renaming
/// the product later does not change history). `serial_number` is set for a
/// serial label.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelJobItem {
    pub product_key: String,
    pub product_name: String,
    pub sku: String,
    pub serial_number: Option<String>,
    pub price_cents: Option<i64>,
    pub copies: u32,
}

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

/// Query of `GET /labels/jobs`. `from`/`to` are RFC 3339 instants (`to` exclusive).
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LabelJobListQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Only jobs that printed this product.
    pub product_key: Option<String>,
    pub action: Option<LabelPrintAction>,
}

/// `GET /labels/jobs` result, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelJobListResponse {
    pub items: Vec<LabelJob>,
    pub pagination: PaginationMeta,
}

/// Whether a product's labels are up to date. `Never`: in stock and never
/// labelled. `Needed`: labelled before, but units entered since are still on
/// the shelf. `Labelled`: everything else that has been labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelCoverageStatus {
    Never,
    Needed,
    Labelled,
}

/// Query of `GET /labels/coverage`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LabelCoverageQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    /// Only products in this state; omitted lists every product.
    pub status: Option<LabelCoverageStatus>,
    /// Matches name, SKU, barcode or category, like the inventory list.
    pub search: Option<String>,
    /// Comma-separated product keys (at most 200): only these products, e.g.
    /// to print a past job again with today's details. Ignores `search`.
    pub product_keys: Option<String>,
}

/// One product with its label state. `status` is `None` for a product that
/// was never labelled and has nothing in stock (nothing to label).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageRow {
    pub product: Product,
    pub status: Option<LabelCoverageStatus>,
    pub last_labelled_at: Option<DateTime<Utc>>,
    /// Stickers of this product in its last print.
    pub last_labelled_copies: Option<u32>,
    /// Units entered after the last label that are still in stock.
    pub received_since_label: i64,
}

/// How many products are in each state (ignores `status`, honours `search`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageCounts {
    pub never: u64,
    pub needed: u64,
    pub labelled: u64,
}

/// `GET /labels/coverage` result: never labelled first, then labels needed,
/// then up to date, each by name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageResponse {
    pub items: Vec<LabelCoverageRow>,
    pub pagination: PaginationMeta,
    pub counts: LabelCoverageCounts,
}
