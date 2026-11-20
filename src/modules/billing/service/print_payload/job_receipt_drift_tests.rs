//! The job receipt payload against the real published schemas of its kind, and what it says for
//! a repair, a print job, a warranty rework and a shop with no terms or pickup warning.

use super::build_job_receipt_data;
use super::schema_drift_tests::assert_matches_schema;
use crate::domain::job_receipts::{JobReceiptPaper, JobReceiptSource};
use crate::domain::print_jobs::PrintJob;
use crate::domain::repairs::Repair;
use crate::domain::settings::ShopProfileResponse;
use serde_json::{Value, json};

const KIND: &str = "job-receipt";

const PAPERS: [JobReceiptPaper; 3] = [
    JobReceiptPaper::A5,
    JobReceiptPaper::Thermal58,
    JobReceiptPaper::Thermal80,
];

fn shop(terms: &str, days: Option<i64>) -> ShopProfileResponse {
    ShopProfileResponse {
        id: "64b0000000000000000000b1".to_string(),
        version: 1,
        legal_name: "Tweek Computers (Pvt) Ltd".to_string(),
        trading_name: "Tweek PC".to_string(),
        address_lines: vec!["45 Main Street".to_string(), "Kandy".to_string()],
        primary_phone: "081 222 3344".to_string(),
        secondary_phone: String::new(),
        email: "hello@tweekpc.lk".to_string(),
        website: "tweekpc.lk".to_string(),
        business_reg_no: String::new(),
        logo_base64: String::new(),
        bank_name: String::new(),
        bank_branch: String::new(),
        account_name: String::new(),
        account_number: String::new(),
        default_warranty_text: String::new(),
        default_footer_text: String::new(),
        receipt_footer_text: String::new(),
        job_receipt_terms_text: terms.to_string(),
        uncollected_after_days: days,
        allow_shared_user_sessions: false,
        allow_staff_multi_device_sessions: false,
    }
}

/// A repair as the API returns it; `overrides` replace fields of the standard default.
fn repair(overrides: Value) -> Repair {
    let mut value = json!({
        "id": "64b0000000000000000000c1",
        "key": "rep_test",
        "ticketNumber": "REP-000123",
        "customerKey": "cust_test",
        "customerName": "Nimal Perera",
        "customerPhone": "077 123 4567",
        "deviceModel": "Samsung Galaxy A15",
        "serialNumber": "356789104512347",
        "issueDescription": "Screen cracked after a fall.",
        "promisedReadyAt": "2026-11-27",
        "isOverdue": false,
        "status": "received",
        "kind": {"kind": "standard"},
        "items": [{
            "key": "rit_1",
            "description": "Replace display",
            "laborCents": 500000,
            "partCents": 1350000,
            "partCostCents": 900000,
            "warranty": {"coverage": {"kind": "none"}}
        }],
        "priceCents": 1850000,
        "costCents": 900000,
        "createdAt": "2026-11-20T09:05:00Z",
        "updatedAt": "2026-11-20T09:05:00Z",
        "version": 1
    });
    for (field, overridden) in overrides.as_object().expect("overrides are an object") {
        value[field] = overridden.clone();
    }
    serde_json::from_value(value).expect("repair fixture deserializes")
}

/// A print job as the API returns it; `overrides` replace fields of the mug default.
fn print_job(overrides: Value) -> PrintJob {
    let mut value = json!({
        "id": "64b0000000000000000000d1",
        "key": "prn_test",
        "ticketNumber": "PRN-000045",
        "customerKey": "cust_test",
        "customerName": "Nimal Perera",
        "customerPhone": "077 123 4567",
        "jobType": "mug",
        "quantity": 20,
        "isOverdue": false,
        "status": "received",
        "estimatedCostCents": 800000,
        "createdAt": "2026-11-20T09:05:00Z",
        "updatedAt": "2026-11-20T09:05:00Z",
        "version": 1
    });
    for (field, overridden) in overrides.as_object().expect("overrides are an object") {
        value[field] = overridden.clone();
    }
    serde_json::from_value(value).expect("print job fixture deserializes")
}

fn data_of(source: &JobReceiptSource, shop: &ShopProfileResponse, paper: JobReceiptPaper) -> Value {
    build_job_receipt_data(source, shop, paper).expect("the payload is built")
}

#[test]
fn a_repair_receipt_matches_every_schema_on_every_paper() {
    let source = JobReceiptSource::from_repair(&repair(json!({})));
    for paper in PAPERS {
        assert_matches_schema(
            KIND,
            &data_of(&source, &shop("Keep this slip.", Some(30)), paper),
        );
    }
}

#[test]
fn a_repair_receipt_is_in_shop_time_with_the_work_and_the_estimate() {
    let source = JobReceiptSource::from_repair(&repair(json!({})));
    let data = data_of(
        &source,
        &shop("Keep this slip.", Some(30)),
        JobReceiptPaper::A5,
    );
    // 09:05 UTC is 14:35 in Sri Lanka.
    assert_eq!(data["receivedOn"], "20 Nov 2026");
    assert_eq!(data["receivedAt"], "14:35");
    assert_eq!(data["promisedReadyOn"], "27 Nov 2026");
    assert_eq!(data["itemLines"], json!(["Replace display"]));
    assert_eq!(data["estimateCents"], 1_850_000);
    assert_eq!(data["qrData"], "REP-000123");
    assert_eq!(data["jobKind"], "repair");
    assert_eq!(data["termsText"], "Keep this slip.");
    assert_eq!(data["uncollectedAfterDays"], 30);
}

#[test]
fn a_repair_with_no_work_yet_has_no_estimate_and_no_promised_day() {
    let source = JobReceiptSource::from_repair(&repair(json!({
        "items": [],
        "priceCents": 0,
        "costCents": 0,
        "promisedReadyAt": null,
        "serialNumber": null
    })));
    let data = data_of(&source, &shop("", None), JobReceiptPaper::Thermal80);
    assert_eq!(data["estimateCents"], Value::Null);
    assert_eq!(data["promisedReadyOn"], Value::Null);
    assert_eq!(data["serialNumber"], "");
    assert_eq!(data["termsText"], "");
    assert_eq!(data["uncollectedAfterDays"], Value::Null);
    for paper in PAPERS {
        assert_matches_schema(KIND, &data_of(&source, &shop("", None), paper));
    }
}

#[test]
fn a_warranty_rework_is_free() {
    let source = JobReceiptSource::from_repair(&repair(json!({
        "kind": {
            "kind": "warranty_rework",
            "claimKey": "wcl_1",
            "claimNumber": "WCL-000001",
            "originalRepairKey": "rep_old",
            "originalTicketNumber": "REP-000100"
        },
        "priceCents": 0
    })));
    let data = data_of(&source, &shop("", None), JobReceiptPaper::A5);
    assert_eq!(data["isWarrantyRework"], true);
    assert_eq!(data["estimateCents"], Value::Null);
    assert_matches_schema(KIND, &data);
}

#[test]
fn a_print_job_receipt_names_the_quantity_and_the_kind_and_matches_every_schema() {
    let source = JobReceiptSource::from_print_job(&print_job(json!({}))).expect("a mug job");
    let data = data_of(&source, &shop("", Some(14)), JobReceiptPaper::Thermal58);
    assert_eq!(data["jobKind"], "print");
    assert_eq!(data["title"], "Mug");
    assert_eq!(data["detail"], "20 × Mug");
    assert_eq!(data["itemLines"], json!([]));
    assert_eq!(data["serialNumber"], "");
    assert_eq!(data["estimateCents"], 800_000);
    assert_eq!(data["qrData"], "PRN-000045");
    for paper in PAPERS {
        assert_matches_schema(KIND, &data_of(&source, &shop("", Some(14)), paper));
    }
}

#[test]
fn a_print_job_quoted_at_nothing_has_no_estimate() {
    let source = JobReceiptSource::from_print_job(&print_job(json!({"estimatedCostCents": 0})))
        .expect("a mug job");
    assert_eq!(
        data_of(&source, &shop("", None), JobReceiptPaper::A5)["estimateCents"],
        Value::Null
    );
}

#[test]
fn a_print_job_stored_with_an_unknown_type_is_refused() {
    let error = JobReceiptSource::from_print_job(&print_job(json!({"jobType": "poster"})))
        .expect_err("an unknown type is not printed");
    assert!(error.to_string().contains("poster"), "{error}");
}
