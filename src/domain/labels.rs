// Pure business types for label printing: a shop-designed label layout and the
// requests that print it. No I/O, no Mongo/Axum types beyond serde/utoipa
// derives. The layout is plain data (absolutely placed elements, in
// millimetres); document-server's "Product Sticker Label" template draws it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
