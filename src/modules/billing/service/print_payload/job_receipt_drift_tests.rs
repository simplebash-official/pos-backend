//! The job receipt payload against the real published schemas of its kind, and what it says for
//! a repair, a print job, a warranty rework, several jobs on one slip and a shop with no terms or
//! pickup warning.

use super::build_job_receipt_data;
use super::schema_drift_tests::assert_matches_schema;
use crate::domain::job_receipts::{JobReceipt, JobReceiptPaper, JobReceiptSource};
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
        "kind": {"kind": "standard"},
        "items": [{
            "key": "pit_1",
            "description": "Mug printing",
            "quantity": 20,
            "source": {"type": "service"},
            "unitPriceCents": 40000,
            "costCents": 0,
            "warranty": {"coverage": {"kind": "none"}}
        }],
        "frames": [],
        "priceCents": 800000,
        "costCents": 0,
        "createdAt": "2026-11-20T09:05:00Z",
        "updatedAt": "2026-11-20T09:05:00Z",
        "version": 1
    });
    for (field, overridden) in overrides.as_object().expect("overrides are an object") {
        value[field] = overridden.clone();
    }
    serde_json::from_value(value).expect("print job fixture deserializes")
}

fn data_of_all(
    sources: &[JobReceiptSource],
    shop: &ShopProfileResponse,
    paper: JobReceiptPaper,
) -> Value {
    let receipt = JobReceipt::new(sources.to_vec()).expect("one customer's jobs");
    build_job_receipt_data(&receipt, shop, paper).expect("the payload is built")
}

fn data_of(source: &JobReceiptSource, shop: &ShopProfileResponse, paper: JobReceiptPaper) -> Value {
    data_of_all(std::slice::from_ref(source), shop, paper)
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
    assert_eq!(data["jobs"][0]["receivedOn"], "20 Nov 2026");
    assert_eq!(data["jobs"][0]["receivedAt"], "14:35");
    assert_eq!(data["jobs"][0]["promisedReadyOn"], "27 Nov 2026");
    assert_eq!(data["jobs"][0]["itemLines"], json!(["Replace display"]));
    assert_eq!(data["jobs"][0]["estimateCents"], 1_850_000);
    assert_eq!(data["jobs"][0]["qrData"], "REP-000123");
    assert_eq!(data["jobs"][0]["jobKind"], "repair");
    assert_eq!(data["termsText"], "Keep this slip.");
    assert_eq!(data["uncollectedAfterDays"], 30);
}

#[test]
fn a_framing_job_lists_the_wanted_frames_before_its_work_and_charges_only_the_work() {
    let job = print_job(json!({
        "jobType": "photo-framing",
        "frames": [
            {"frameSizeKey": "pfs_1", "size": "8x10", "widthHundredthsInch": 75, "quantity": 2},
            {"frameSizeKey": "pfs_2", "size": "A4", "widthHundredthsInch": 100, "quantity": 1}
        ]
    }));
    let source = JobReceiptSource::from_print_job(&job).expect("a framing job");
    for paper in PAPERS {
        assert_matches_schema(KIND, &data_of(&source, &shop("", None), paper));
    }
    let data = data_of(&source, &shop("", None), JobReceiptPaper::A5);
    assert_eq!(
        data["jobs"][0]["itemLines"],
        json!([
            "Frame 8x10 · 0.75 inch x 2",
            "Frame A4 · 1 inch x 1",
            "Mug printing x 20"
        ])
    );
    assert_eq!(data["jobs"][0]["estimateCents"], 800_000);
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
    assert_eq!(data["jobs"][0]["estimateCents"], Value::Null);
    assert_eq!(data["jobs"][0]["promisedReadyOn"], Value::Null);
    assert_eq!(data["jobs"][0]["serialNumber"], "");
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
    assert_eq!(data["jobs"][0]["isWarrantyRework"], true);
    assert_eq!(data["jobs"][0]["estimateCents"], Value::Null);
    assert_matches_schema(KIND, &data);
}

#[test]
fn a_print_job_receipt_names_the_quantity_and_the_kind_and_matches_every_schema() {
    let source = JobReceiptSource::from_print_job(&print_job(json!({}))).expect("a mug job");
    let data = data_of(&source, &shop("", Some(14)), JobReceiptPaper::Thermal58);
    assert_eq!(data["jobs"][0]["jobKind"], "print");
    assert_eq!(data["jobs"][0]["title"], "Mug");
    assert_eq!(data["jobs"][0]["detail"], "20 × Mug");
    assert_eq!(data["jobs"][0]["itemLines"], json!(["Mug printing x 20"]));
    assert_eq!(data["jobs"][0]["serialNumber"], "");
    assert_eq!(data["jobs"][0]["estimateCents"], 800_000);
    assert_eq!(data["jobs"][0]["qrData"], "PRN-000045");
    for paper in PAPERS {
        assert_matches_schema(KIND, &data_of(&source, &shop("", Some(14)), paper));
    }
}

#[test]
fn a_print_job_with_no_items_has_no_estimate() {
    let source =
        JobReceiptSource::from_print_job(&print_job(json!({"items": [], "priceCents": 0})))
            .expect("a mug job");
    let data = data_of(&source, &shop("", None), JobReceiptPaper::A5);
    assert_eq!(data["jobs"][0]["estimateCents"], Value::Null);
    assert_eq!(data["jobs"][0]["itemLines"], json!([]));
}

#[test]
fn a_print_rework_is_free() {
    let source = JobReceiptSource::from_print_job(&print_job(json!({
        "kind": {
            "kind": "warranty_rework",
            "claimKey": "wcl_1",
            "claimNumber": "WCL-000001",
            "originalPrintJobKey": "prn_old",
            "originalTicketNumber": "PRN-000010"
        },
        "priceCents": 0
    })))
    .expect("a mug job");
    let data = data_of(&source, &shop("", None), JobReceiptPaper::A5);
    assert_eq!(data["jobs"][0]["isWarrantyRework"], true);
    assert_eq!(data["jobs"][0]["estimateCents"], Value::Null);
    assert_matches_schema(KIND, &data);
}

#[test]
fn a_print_job_stored_with_an_unknown_type_is_refused() {
    let error = JobReceiptSource::from_print_job(&print_job(json!({"jobType": "poster"})))
        .expect_err("an unknown type is not printed");
    assert!(error.to_string().contains("poster"), "{error}");
}

#[test]
fn several_jobs_share_a_slip_in_order_with_their_total() {
    let sources = [
        JobReceiptSource::from_print_job(&print_job(json!({}))).expect("a mug job"),
        JobReceiptSource::from_repair(&repair(json!({}))),
    ];
    for paper in PAPERS {
        let data = data_of_all(&sources, &shop("Keep this slip.", Some(30)), paper);
        assert_eq!(data["jobs"][0]["ticketNumber"], "PRN-000045");
        assert_eq!(data["jobs"][1]["ticketNumber"], "REP-000123");
        assert_eq!(data["customerName"], "Nimal Perera");
        assert_eq!(data["estimateTotalCents"], 2_650_000);
        assert_matches_schema(KIND, &data);
    }
}

#[test]
fn a_total_needs_every_charged_job_priced_and_leaves_free_work_out() {
    let priced = JobReceiptSource::from_repair(&repair(json!({})));
    let unpriced =
        JobReceiptSource::from_print_job(&print_job(json!({"items": [], "priceCents": 0})))
            .expect("a mug job");
    let rework = JobReceiptSource::from_print_job(&print_job(json!({
        "kind": {
            "kind": "warranty_rework",
            "claimKey": "wcl_1",
            "claimNumber": "WCL-000001",
            "originalPrintJobKey": "prn_old",
            "originalTicketNumber": "PRN-000010"
        },
        "priceCents": 0
    })))
    .expect("a mug job");
    let total = |sources: &[JobReceiptSource]| {
        data_of_all(sources, &shop("", None), JobReceiptPaper::A5)["estimateTotalCents"].clone()
    };
    assert_eq!(total(std::slice::from_ref(&priced)), Value::Null);
    assert_eq!(total(&[priced.clone(), unpriced]), Value::Null);
    assert_eq!(total(&[priced.clone(), rework.clone()]), json!(1_850_000));
    assert_eq!(total(&[rework.clone(), rework]), Value::Null);
}

#[test]
fn jobs_of_different_customers_never_share_a_slip() {
    let error = JobReceipt::new(vec![
        JobReceiptSource::from_repair(&repair(json!({}))),
        JobReceiptSource::from_repair(&repair(json!({"customerKey": "cust_other"}))),
    ])
    .expect_err("two customers");
    assert!(error.to_string().contains("one customer"), "{error}");
}
