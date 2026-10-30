use super::service_validate::MAX_CUSTOM_TEXT_CHARS;
use super::service_validate::MAX_ELEMENTS;
use super::service_validate::MAX_NAME_CHARS;
use super::service_validate::check_code_fits;
use super::service_validate::code128_modules;
use super::service_validate::qr_modules;
use super::service_validate::validate_template;
use crate::core::error::{AppError, AppResult};
use crate::domain::labels::{LabelElement, LabelTemplateInput, TextSource};

use crate::domain::labels::TextAlign;

fn text(id: &str, source: TextSource, custom: Option<&str>) -> LabelElement {
    LabelElement::Text {
        id: id.into(),
        source,
        custom_text: custom.map(str::to_string),
        x_mm: 1.0,
        y_mm: 1.0,
        width_mm: 20.0,
        height_mm: 5.0,
        font_size_pt: 8.0,
        bold: false,
        align: TextAlign::Left,
    }
}

fn barcode(width: f64, height: f64, show_value: bool) -> LabelElement {
    LabelElement::Barcode {
        id: "b".into(),
        x_mm: 1.0,
        y_mm: 8.0,
        width_mm: width,
        height_mm: height,
        show_value,
        font_size_pt: 6.0,
    }
}

fn input(elements: Vec<LabelElement>) -> LabelTemplateInput {
    LabelTemplateInput {
        name: "Shelf".into(),
        width_mm: 50.0,
        height_mm: 30.0,
        elements,
    }
}

fn message(result: AppResult<()>) -> String {
    match result {
        Err(AppError::Validation { message, .. }) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn a_well_formed_design_passes() {
    let design = input(vec![
        text("n", TextSource::ProductName, None),
        barcode(46.0, 15.0, true),
    ]);
    assert!(validate_template(&design).is_ok());
}

#[test]
fn name_size_and_item_count_are_bounded() {
    let ok = vec![text("n", TextSource::Sku, None)];
    let mut blank = input(ok.clone());
    blank.name = "   ".into();
    assert!(message(validate_template(&blank)).contains("name"));

    let mut long = input(ok.clone());
    long.name = "x".repeat(MAX_NAME_CHARS + 1);
    assert!(message(validate_template(&long)).contains("at most"));

    let mut narrow = input(ok.clone());
    narrow.width_mm = 19.9;
    assert!(message(validate_template(&narrow)).contains("width"));

    let mut tall = input(ok.clone());
    tall.height_mm = 200.1;
    assert!(message(validate_template(&tall)).contains("height"));

    assert!(message(validate_template(&input(vec![]))).contains("at least one"));

    let many: Vec<_> = (0..=MAX_ELEMENTS)
        .map(|i| text(&format!("t{i}"), TextSource::Sku, None))
        .collect();
    assert!(message(validate_template(&input(many))).contains("at most"));
}

#[test]
fn items_must_have_unique_ids_and_stay_inside_the_label() {
    let dup = input(vec![
        text("a", TextSource::Sku, None),
        text("a", TextSource::Code, None),
    ]);
    assert!(message(validate_template(&dup)).contains("same id"));

    let mut outside = text("o", TextSource::Sku, None);
    if let LabelElement::Text { width_mm, .. } = &mut outside {
        *width_mm = 50.0; // x = 1 + 50 > 50
    }
    assert!(message(validate_template(&input(vec![outside]))).contains("edge"));

    // Exactly touching the edge is fine, despite float noise.
    let mut flush = text("f", TextSource::Sku, None);
    if let LabelElement::Text { x_mm, width_mm, .. } = &mut flush {
        *x_mm = 0.1 + 0.2;
        *width_mm = 50.0 - 0.3;
    }
    assert!(validate_template(&input(vec![flush])).is_ok());

    let mut nan = text("n", TextSource::Sku, None);
    if let LabelElement::Text { x_mm, .. } = &mut nan {
        *x_mm = f64::NAN;
    }
    assert!(validate_template(&input(vec![nan])).is_err());
}

#[test]
fn custom_text_is_required_exactly_for_the_custom_source() {
    let missing = input(vec![text("c", TextSource::CustomText, None)]);
    assert!(message(validate_template(&missing)).contains("needs its text"));
    let blank = input(vec![text("c", TextSource::CustomText, Some("  "))]);
    assert!(message(validate_template(&blank)).contains("cannot be empty"));
    let stray = input(vec![text("c", TextSource::Sku, Some("hello"))]);
    assert!(message(validate_template(&stray)).contains("custom text item"));
    let long = "x".repeat(MAX_CUSTOM_TEXT_CHARS + 1);
    let too_long = input(vec![text("c", TextSource::CustomText, Some(&long))]);
    assert!(message(validate_template(&too_long)).contains("at most"));
    let fine = input(vec![text("c", TextSource::CustomText, Some("Warranty"))]);
    assert!(validate_template(&fine).is_ok());
}

#[test]
fn font_sizes_are_bounded() {
    let mut tiny = text("t", TextSource::Sku, None);
    if let LabelElement::Text { font_size_pt, .. } = &mut tiny {
        *font_size_pt = 3.9;
    }
    assert!(message(validate_template(&input(vec![tiny]))).contains("Text size"));
}

#[test]
fn a_barcode_must_keep_readable_bars_below_its_caption() {
    // 6pt caption reserves (6 * 1.25 + 2) * 0.3528 = 3.35mm.
    assert!(validate_template(&input(vec![barcode(46.0, 6.5, true)])).is_ok());
    assert!(
        message(validate_template(&input(vec![barcode(46.0, 5.9, true)]))).contains("too short")
    );
    assert!(validate_template(&input(vec![barcode(46.0, 3.0, false)])).is_ok());
    assert!(
        message(validate_template(&input(vec![barcode(46.0, 2.9, false)]))).contains("too short")
    );
}

// Real Zint output measured at 127/137/82 bars-only modules for these codes
// (+20 quiet); the estimate must not fall below the real width.
#[test]
fn code128_estimate_is_never_below_measured_widths() {
    assert!(code128_modules("2000000000015") >= 147);
    assert!(code128_modules("356938035643809") >= 157);
    assert!(code128_modules("12345678") >= 102);
    assert!(code128_modules("2000000000") >= 112);
    assert!(code128_modules("SN-ABC-12345") >= 181);
    assert!(code128_modules("A1B2C3D4E5F6G7H8") >= 236);
    // ...and it does not wildly overshoot for digits.
    assert!(code128_modules("2000000000015") <= 160);
}

#[test]
fn qr_estimate_follows_the_byte_mode_versions() {
    // Symbol size plus the template's 2-module quiet zone on each side.
    assert_eq!(qr_modules("2000000000015"), 25); // v1: 21 + 4
    assert_eq!(qr_modules("356938035643809"), 29); // v2: 25 + 4
    assert_eq!(qr_modules(&"A".repeat(30)), 33); // v3: 29 + 4
    assert_eq!(qr_modules(&"A".repeat(64)), 41); // v5: 37 + 4
}

#[test]
fn narrow_barcodes_and_small_qr_codes_are_refused_at_print_time() {
    let wide = [barcode(46.0, 15.0, true)];
    assert!(check_code_fits(&wide, "2000000000015", "Cable").is_ok());

    let narrow = [barcode(30.0, 15.0, true)];
    let err = check_code_fits(&narrow, "2000000000015", "Cable").unwrap_err();
    assert!(err.to_string().contains("too narrow"), "{err}");

    let qr = |size: f64| {
        [LabelElement::Qr {
            id: "q".into(),
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: size,
            height_mm: size,
        }]
    };
    assert!(check_code_fits(&qr(10.0), "2000000000015", "Cable").is_ok());
    let err = check_code_fits(&qr(6.0), "2000000000015", "Cable").unwrap_err();
    assert!(err.to_string().contains("too small"), "{err}");
}
