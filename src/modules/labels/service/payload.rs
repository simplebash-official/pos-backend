// Builds the JSON handed to document-server's "Product Sticker Label" template
// (slug `product-label`, see `clients::document_server`). The contract is that
// template's `lbl_temp_*.schema.json`; `schema_drift_tests` checks this builder
// against the real sidecar so the two cannot drift apart.

use serde::Serialize;
use serde_json::Value;

use crate::{
    core::error::{AppError, AppResult},
    domain::labels::LabelElement,
};

/// One sticker to print, printed `copies` times. `price_cents` is `None` only
/// when the design has no price element.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Sticker {
    /// Which product this sticker is for; kept for the print history, never
    /// sent to document-server.
    #[serde(skip)]
    pub product_key: String,
    /// The unit's serial number on a serial label (history only, not sent).
    #[serde(skip)]
    pub serial_number: Option<String>,
    pub name: String,
    pub sku: String,
    pub category: String,
    pub price_cents: Option<i64>,
    pub code: String,
    pub copies: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LabelPayload<'a> {
    width_mm: f64,
    height_mm: f64,
    shop_name: &'a str,
    elements: &'a [LabelElement],
    labels: &'a [Sticker],
}

/// The template's `sys.inputs`: the layout plus the stickers to draw with it.
pub(crate) fn build_label_data(
    width_mm: f64,
    height_mm: f64,
    shop_name: &str,
    elements: &[LabelElement],
    stickers: &[Sticker],
) -> AppResult<Value> {
    serde_json::to_value(LabelPayload {
        width_mm,
        height_mm,
        shop_name,
        elements,
        labels: stickers,
    })
    .map_err(|e| AppError::internal(format!("Failed to build the label payload: {e}")))
}

#[cfg(test)]
mod schema_drift_tests {
    use super::*;
    use crate::modules::labels::service::starters;

    /// The real sidecar schema of the "Product Sticker Label" template from the
    /// sibling document-server checkout. Panics (not skips) when it is missing:
    /// unlike the document kinds, this contract has no other guard.
    fn load_schema() -> Value {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../document-server/templates/labels"
        );
        for entry in std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("no document-server checkout at {dir}: {e}"))
            .flatten()
        {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(".schema.json") {
                continue;
            }
            let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
                .expect("sidecar schema is valid JSON");
            if schema["title"] == "Product Sticker Label" {
                return schema;
            }
        }
        panic!("no schema titled 'Product Sticker Label' in {dir}");
    }

    fn assert_valid(schema: &Value, payload: &Value, what: &str) {
        let validator = jsonschema::validator_for(schema).expect("published schema compiles");
        let violations: Vec<String> = validator
            .iter_errors(payload)
            .map(|e| format!("at '{}': {e}", e.instance_path()))
            .collect();
        assert!(
            violations.is_empty(),
            "{what} drifted from the label template's schema:\n  {}",
            violations.join("\n  ")
        );
    }

    fn sticker(price: Option<i64>) -> Sticker {
        Sticker {
            product_key: "prod_sample".to_string(),
            serial_number: None,
            name: "USB-C Cable".into(),
            sku: "CAB-USB-0042".into(),
            category: "Cables".into(),
            price_cents: price,
            code: "2000000000015".into(),
            copies: 3,
        }
    }

    #[test]
    fn every_starter_payload_satisfies_the_published_schema() {
        let schema = load_schema();
        for template in starters::all() {
            let payload = build_label_data(
                template.width_mm,
                template.height_mm,
                "SimpleBash Mobile",
                &template.elements,
                &[sticker(Some(125_000)), sticker(None)],
            )
            .unwrap();
            assert_valid(&schema, &payload, &template.name);
        }
    }

    #[test]
    fn the_payload_has_the_shape_the_template_reads() {
        let template = starters::all().remove(0);
        let payload = build_label_data(
            template.width_mm,
            template.height_mm,
            "Shop",
            &template.elements,
            &[sticker(Some(100))],
        )
        .unwrap();
        assert_eq!(payload["widthMm"], 50.0);
        assert_eq!(payload["shopName"], "Shop");
        assert_eq!(payload["labels"][0]["priceCents"], 100);
        assert_eq!(payload["labels"][0]["copies"], 3);
        // A non-custom text element carries an explicit null, not a missing key.
        assert!(payload["elements"][0]["customText"].is_null());
        assert_eq!(payload["elements"][0]["type"], "text");
        assert_eq!(payload["elements"][0]["source"], "productName");
        assert_eq!(payload["elements"][1]["type"], "barcode");
    }
}
