use super::service_jobs::LastLabel;
use super::service_jobs::coverage_status;
use super::service_jobs::last_labels;
use super::service_jobs::validate_job_request;

use crate::domain::labels::{
    CreateLabelJobRequest, LabelCoverageStatus, LabelPrintAction, LabelPrintMethod,
};

use crate::modules::labels::repository;

use chrono::Utc;
use mongodb::bson::DateTime as BsonDateTime;

fn request(
    action: LabelPrintAction,
    method: Option<LabelPrintMethod>,
    printer: Option<&str>,
    dpi: Option<u32>,
) -> CreateLabelJobRequest {
    CreateLabelJobRequest {
        template_key: "starter_x".into(),
        items: Vec::new(),
        action,
        method,
        printer_name: printer.map(str::to_string),
        dpi,
    }
}

#[test]
fn a_job_request_names_exactly_what_its_action_needs() {
    use LabelPrintAction::{Download, Print};
    use LabelPrintMethod::{Direct, System};
    let ok = [
        request(Download, None, None, None),
        request(Print, Some(System), None, Some(203)),
        request(Print, Some(Direct), Some("Zebra ZD220"), Some(300)),
    ];
    for r in ok {
        assert!(validate_job_request(&r).is_ok(), "{r:?}");
    }
    let bad = [
        request(Download, Some(System), None, None),
        request(Download, None, None, Some(203)),
        request(Print, None, None, Some(203)),
        request(Print, Some(System), None, None),
        request(Print, Some(System), None, Some(150)),
        request(Print, Some(System), Some("Zebra"), Some(203)),
        request(Print, Some(Direct), None, Some(203)),
        request(Print, Some(Direct), Some("   "), Some(203)),
        request(Print, Some(Direct), Some(&"x".repeat(201)), Some(203)),
    ];
    for r in bad {
        assert!(validate_job_request(&r).is_err(), "{r:?}");
    }
}

fn entry(job: &str, product: &str, copies: u32, at: i64) -> repository::LabelledEntry {
    repository::LabelledEntry {
        job_key: job.into(),
        product_key: product.into(),
        copies,
        created_at: BsonDateTime::from_millis(at),
    }
}

#[test]
fn the_last_label_is_the_newest_job_with_its_groups_added_up() {
    // Newest job first, as the repository returns them.
    let last = last_labels(vec![
        entry("lpj_2", "prod_a", 1, 2_000),
        entry("lpj_2", "prod_a", 1, 2_000),
        entry("lpj_2", "prod_b", 4, 2_000),
        entry("lpj_1", "prod_a", 9, 1_000),
        entry("lpj_1", "prod_c", 2, 1_000),
    ]);
    assert_eq!(last["prod_a"].copies, 2);
    assert_eq!(last["prod_a"].at.timestamp_millis(), 2_000);
    assert_eq!(last["prod_b"].copies, 4);
    assert_eq!(last["prod_c"].copies, 2);
    assert_eq!(last["prod_c"].at.timestamp_millis(), 1_000);
}

#[test]
fn coverage_status_follows_stock_and_history() {
    let label = Some(LastLabel {
        at: Utc::now(),
        copies: 1,
    });
    assert_eq!(
        coverage_status(5, None, 0),
        Some(LabelCoverageStatus::Never)
    );
    assert_eq!(coverage_status(0, None, 0), None);
    assert_eq!(
        coverage_status(5, label, 3),
        Some(LabelCoverageStatus::Needed)
    );
    assert_eq!(
        coverage_status(5, label, 0),
        Some(LabelCoverageStatus::Labelled)
    );
    assert_eq!(
        coverage_status(0, label, 0),
        Some(LabelCoverageStatus::Labelled)
    );
}
