//! Test-harness pieces: mock document server.

use axum::Router;
use serde_json::json;

/// Spawns a lightweight in-process mock document-server on a free port so
/// integration tests can exercise document rendering endpoints without
/// requiring a live external document-server process.
///
/// Serves the same minimal `dataSchema` contracts the real service publishes
/// for its three billing-relevant templates — enough for the client's
/// pre-validation to run for real against realistic payloads.
pub async fn start_mock_document_server() -> String {
    start_mock_document_server_with_templates(mock_templates_json()).await
}

/// Same mock, but publishing a deliberately unsatisfiable schema for the
/// A4 Invoice (requires a field no payload builder will ever send). Used
/// by the pre-validation failure-path test, which must not disturb the
/// shared default mock other tests rely on.
#[allow(dead_code)]
pub async fn start_strict_mock_document_server() -> String {
    let mut templates = mock_templates_json();
    for template in templates["data"]["templates"]
        .as_array_mut()
        .expect("default mock template list")
        .iter_mut()
    {
        if template["description"] == "A4 Invoice" {
            template["dataSchema"]["required"] =
                json!(["invoiceNumber", "__never_sent_by_backend"]);
        }
    }
    start_mock_document_server_with_templates(templates).await
}

/// One template entry as `GET /api/templates` publishes it (the latest
/// published version of a design family). `kind` goes into the schema as
/// `x-document-kind`; a bundled template is its own family.
pub(super) fn mock_template(
    key: &str,
    name: &str,
    title: &str,
    kind: &str,
    contract: serde_json::Value,
    is_default: bool,
    sample: serde_json::Value,
) -> serde_json::Value {
    json!({
        "key": key,
        "name": name,
        "family": name,
        "version": 1,
        "type": "document",
        "kind": kind,
        "description": title,
        "status": "published",
        "scope": "global",
        "isDefault": is_default,
        "options": [],
        "sha256": "0".repeat(64),
        "minAppVersion": null,
        "dataSchema": {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": title,
            "x-document-kind": kind,
            "type": "object",
            "additionalProperties": true,
            "required": contract["required"],
            "properties": contract["properties"]
        },
        "data": sample,
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z"
    })
}

/// A published custom design: a private package of the A4 invoice kind whose
/// schema requires the `options` object its two controls fill.
pub fn mock_private_a4_invoice() -> serde_json::Value {
    let mut template = mock_template(
        "tpl_client_invoice",
        "doc_temp_ClientInvoiceV2",
        "Client Invoice",
        "a4-invoice",
        json!({
            "required": ["invoiceNumber", "options"],
            "properties": {
                "invoiceNumber": { "type": "string" },
                "options": {
                    "type": "object",
                    "required": ["showSign", "accent"],
                    "properties": {
                        "showSign": { "type": "boolean" },
                        "accent": { "type": "string" }
                    }
                }
            }
        }),
        false,
        json!({ "invoiceNumber": "INV-CLIENT", "options": { "showSign": true, "accent": "blue" } }),
    );
    template["family"] = json!("fam_ClientInvoice00");
    template["version"] = json!(2);
    template["scope"] = json!("private");
    template["sha256"] = json!("a".repeat(64));
    template["options"] = json!([
        { "key": "showSign", "type": "toggle", "label": "Signature box", "default": true },
        {
            "key": "accent", "type": "select", "label": "Accent colour", "default": "blue",
            "choices": [{ "value": "blue", "label": "Blue" }, { "value": "red", "label": "Red" }]
        }
    ]);
    template
}

pub(super) fn mock_templates_json() -> serde_json::Value {
    let a4 = json!({
        "required": ["invoiceNumber"],
        "properties": {
            "invoiceNumber": { "type": "string" },
            "logoUrl": { "type": "string" }
        }
    });
    let receipt = json!({
        "required": ["paperWidthMm", "invoiceNumber"],
        "properties": {
            "paperWidthMm": { "type": "integer", "enum": [58, 80] },
            "invoiceNumber": { "type": "string" }
        }
    });
    let sample_logo = "https://example.invalid/logo.png";
    // The label template is typed `label`, like the real one.
    let mut product_label = mock_template(
        "tpl_product_label",
        "lbl_temp_qklcWIolwoFFN3xk",
        "Product Sticker Label",
        "product-label",
        json!({
            "required": ["widthMm", "heightMm", "shopName", "elements", "labels"],
            "properties": {
                "widthMm": { "type": "number" },
                "heightMm": { "type": "number" },
                "shopName": { "type": "string" },
                "elements": { "type": "array", "minItems": 1 },
                "labels": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["name", "sku", "category", "priceCents", "code", "copies"],
                        "properties": {
                            "priceCents": { "type": ["integer", "null"] },
                            "code": { "type": "string", "minLength": 1 },
                            "copies": { "type": "integer", "minimum": 1 }
                        }
                    }
                }
            }
        }),
        true,
        json!({}),
    );
    product_label["type"] = json!("label");
    json!({
        "success": true,
        "message": "Templates fetched",
        "data": {
            "templates": [
                mock_template(
                    "tpl_a4_invoice",
                    "doc_temp_vEf0Y7jQHQj2rIuO",
                    "A4 Invoice",
                    "a4-invoice",
                    a4.clone(),
                    true,
                    json!({ "invoiceNumber": "INV-CLASSIC", "logoUrl": sample_logo }),
                ),
                mock_template(
                    "tpl_a4_invoice_modern",
                    "doc_temp_ZahcoYg8xCQjuc8c",
                    "A4 Invoice Modern",
                    "a4-invoice",
                    a4,
                    false,
                    json!({ "invoiceNumber": "INV-MODERN", "logoUrl": sample_logo }),
                ),
                mock_private_a4_invoice(),
                mock_template(
                    "tpl_thermal_receipt",
                    "doc_temp_4pz79z5iba7TcEIp",
                    "Thermal Receipt",
                    "thermal-receipt",
                    receipt.clone(),
                    true,
                    json!({ "paperWidthMm": 80, "invoiceNumber": "INV-RECEIPT" }),
                ),
                mock_template(
                    "tpl_thermal_receipt_compact",
                    "doc_temp_CompactReceipt00",
                    "Compact Receipt",
                    "thermal-receipt",
                    receipt,
                    false,
                    json!({ "paperWidthMm": 58, "invoiceNumber": "INV-COMPACT" }),
                ),
                mock_template(
                    "tpl_credit_note",
                    "doc_temp_5DHl8hUQTX3oLBSR",
                    "Credit Note",
                    "credit-note",
                    json!({
                        "required": ["creditNoteNumber"],
                        "properties": { "creditNoteNumber": { "type": "string" } }
                    }),
                    true,
                    json!({ "creditNoteNumber": "CN-SAMPLE" }),
                ),
                mock_template(
                    "tpl_credit_note_compact",
                    "doc_temp_CompactCredit000",
                    "Credit Note Compact",
                    "credit-note",
                    json!({
                        "required": ["creditNoteNumber"],
                        "properties": { "creditNoteNumber": { "type": "string" } }
                    }),
                    false,
                    json!({ "creditNoteNumber": "CN-COMPACT" }),
                ),
                mock_template(
                    "tpl_quotation",
                    "doc_temp_Qu0tati0nV1Std00",
                    "Quotation",
                    "quotation",
                    json!({
                        "required": ["quotationNumber", "validUntil", "items"],
                        "properties": {
                            "quotationNumber": { "type": "string" },
                            "validUntil": { "type": "string" },
                            "items": { "type": "array" }
                        }
                    }),
                    true,
                    json!({ "quotationNumber": "QUO-SAMPLE", "validUntil": "01 Jan 2030", "items": [] }),
                ),
                product_label,
                mock_template(
                    "tpl_analytics_report",
                    "doc_temp_An1yT1csRep0rtV1",
                    "Analytics Report",
                    "analytics-report",
                    json!({
                        "required": ["generatedAt", "periodLabel", "granularityLabel", "kpis", "timeseries"],
                        "properties": {
                            "generatedAt": { "type": "string" },
                            "periodLabel": { "type": "string" },
                            "granularityLabel": { "type": "string" },
                            "kpis": { "type": "array" },
                            "timeseries": { "type": "object" }
                        }
                    }),
                    true,
                    json!({}),
                ),
                mock_template(
                    "tpl_z_report",
                    "doc_temp_ZRep0rtV1Std000",
                    "Z-Report",
                    "z-report",
                    json!({
                        "required": ["sessionNumber", "terminalId", "cashierName"],
                        "properties": {
                            "sessionNumber": { "type": "string" },
                            "terminalId": { "type": "string" },
                            "cashierName": { "type": "string" }
                        }
                    }),
                    true,
                    json!({ "sessionNumber": "SES-000042", "terminalId": "TERM-01", "cashierName": "Kasun Perera" }),
                ),
                mock_template(
                    "tpl_warranty_card",
                    "doc_temp_Wrnty0Card0V1Std",
                    "Warranty Card",
                    "warranty-card",
                    json!({
                        "required": ["paper", "warrantyNumber", "itemName", "coverageLabel", "validUntil"],
                        "properties": {
                            "paper": { "type": "string", "enum": ["a5", "thermal-58", "thermal-80"] },
                            "warrantyNumber": { "type": "string" },
                            "itemName": { "type": "string" },
                            "coverageLabel": { "type": "string" },
                            "validUntil": { "type": ["string", "null"] }
                        }
                    }),
                    true,
                    json!({ "paper": "a5", "warrantyNumber": "WTY-000001", "itemName": "Phone", "coverageLabel": "12 months", "validUntil": "02 Nov 2027" }),
                ),
            ]
        }
    })
}

pub(super) async fn start_mock_document_server_with_templates(
    templates_response: serde_json::Value,
) -> String {
    use axum::Json;
    use axum::body::Body;
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};

    let mock_app = Router::new()
        .route(
            "/api/health",
            get(|| async {
                Json(json!({
                    "success": true,
                    "message": "Service is healthy",
                    "data": { "status": "ok" }
                }))
            }),
        )
        .route(
            "/api/templates",
            get(move |headers: HeaderMap| {
                let templates_response = templates_response.clone();
                async move {
                    // Like the real service: the list needs the internal key.
                    if headers.get("X-Internal-Api-Key").is_none_or(|v| v.is_empty()) {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({
                                "success": false,
                                "code": "INTERNAL_API_KEY_INVALID",
                                "message": "missing api key"
                            })),
                        );
                    }
                    (StatusCode::OK, Json(templates_response))
                }
            }),
        )
        .route(
            "/api/packages/{name}/archive",
            get(|Path(name): Path<String>, headers: HeaderMap| async move {
                if headers.get("X-Internal-Api-Key").is_none_or(|v| v.is_empty()) {
                    return (StatusCode::UNAUTHORIZED, Body::empty()).into_response();
                }
                if name != "doc_temp_ClientInvoiceV2" {
                    return (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/json")],
                        Body::from(r#"{"success":false,"code":"PACKAGE_NOT_FOUND","message":"Package not found"}"#),
                    )
                        .into_response();
                }
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/zip")],
                    Body::from(format!("PK mock archive of {name}")),
                )
                    .into_response()
            }),
        )
        .route(
            "/api/render/{template_key}",
            post(
                |Path(template_key): Path<String>,
                 headers: HeaderMap,
                 body: axum::body::Bytes| async move {
                    if let Some(auth) = headers.get("X-Internal-Api-Key")
                        && auth.is_empty()
                    {
                        return (
                            StatusCode::UNAUTHORIZED,
                            [(header::CONTENT_TYPE, "application/json")],
                            Body::from(
                                r#"{"status":"error","code":"UNAUTHORIZED","message":"missing api key"}"#,
                            ),
                        );
                    }
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/pdf")],
                        Body::from(
                            // Tests read which template rendered, and whether the
                            // payload still carried a `logoUrl`, from this text.
                            format!(
                                "%PDF-1.7 mock rendered PDF for template {template_key} logoUrl={} body={}",
                                String::from_utf8_lossy(&body).contains("\"logoUrl\""),
                                String::from_utf8_lossy(&body)
                            )
                            .into_bytes(),
                        ),
                    )
                },
            ),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock document server");
    let addr = listener.local_addr().expect("failed to get local addr");
    tokio::spawn(async move {
        axum::serve(listener, mock_app).await.ok();
    });

    format!("http://{}", addr)
}
