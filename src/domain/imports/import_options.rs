use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Optional processing switches submitted with a batch import.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    /// If true, rows without a barcode will have a valid EAN-13 barcode auto-generated.
    #[serde(default)]
    pub auto_generate_barcodes: bool,
    /// If true and a referenced category/subcategory does not exist, it is created automatically.
    #[serde(default)]
    pub auto_create_categories: bool,
}
