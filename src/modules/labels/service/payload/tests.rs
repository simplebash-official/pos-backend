use super::service_payload::Sticker;
use super::service_payload::build_label_data;

use serde_json::Value;

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
