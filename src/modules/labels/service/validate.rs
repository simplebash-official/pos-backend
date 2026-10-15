// Rules a label design must satisfy, and the "will this scan?" checks run
// against the real codes at print time. Pure functions: no I/O.

use crate::{
    core::error::{AppError, AppResult},
    domain::labels::{LabelElement, LabelTemplateInput, TextSource},
};

pub(crate) const MIN_WIDTH_MM: f64 = 20.0;
pub(crate) const MAX_WIDTH_MM: f64 = 120.0;
pub(crate) const MIN_HEIGHT_MM: f64 = 10.0;
pub(crate) const MAX_HEIGHT_MM: f64 = 200.0;
pub(crate) const MAX_ELEMENTS: usize = 30;
pub(crate) const MAX_NAME_CHARS: usize = 60;
pub(crate) const MAX_ID_CHARS: usize = 64;
pub(crate) const MAX_CUSTOM_TEXT_CHARS: usize = 200;
pub(crate) const MIN_FONT_PT: f64 = 4.0;
pub(crate) const MAX_FONT_PT: f64 = 72.0;
/// The smallest bar/dot a label printer prints reliably is 2 dots at 203 DPI.
pub(crate) const MIN_MODULE_MM: f64 = 0.25;
/// A barcode's bars must stay this tall (excluding its caption) to be read.
const MIN_BARCODE_BARS_MM: f64 = 3.0;
const PT_TO_MM: f64 = 25.4 / 72.0;
/// Geometry tolerance so a design snapped to a 0.1mm grid is never rejected by float noise.
const EPSILON_MM: f64 = 0.001;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::validation(message)
}

fn finite_in(value: f64, min: f64, max: f64) -> bool {
    value.is_finite() && value >= min && value <= max
}

/// Rejects a design that cannot be stored or rendered. Messages are written for
/// the shop owner who built the design, not for a developer.
pub(crate) fn validate_template(input: &LabelTemplateInput) -> AppResult<()> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(invalid("Give the label design a name."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "The design name can be at most {MAX_NAME_CHARS} characters."
        )));
    }
    if !finite_in(input.width_mm, MIN_WIDTH_MM, MAX_WIDTH_MM) {
        return Err(invalid(format!(
            "The label width must be between {MIN_WIDTH_MM} and {MAX_WIDTH_MM} mm."
        )));
    }
    if !finite_in(input.height_mm, MIN_HEIGHT_MM, MAX_HEIGHT_MM) {
        return Err(invalid(format!(
            "The label height must be between {MIN_HEIGHT_MM} and {MAX_HEIGHT_MM} mm."
        )));
    }
    if input.elements.is_empty() {
        return Err(invalid("Add at least one item to the label."));
    }
    if input.elements.len() > MAX_ELEMENTS {
        return Err(invalid(format!(
            "A label can hold at most {MAX_ELEMENTS} items."
        )));
    }

    let mut seen: Vec<&str> = Vec::with_capacity(input.elements.len());
    for element in &input.elements {
        let (id, x, y, width, height) = element.frame();
        if id.is_empty() || id.chars().count() > MAX_ID_CHARS {
            return Err(invalid(
                "Every label item needs an id of 1 to 64 characters.",
            ));
        }
        if seen.contains(&id) {
            return Err(invalid("Two items on the label share the same id."));
        }
        seen.push(id);

        if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) {
            return Err(invalid("A label item has an invalid position or size."));
        }
        let minimum_size = if matches!(element, LabelElement::Line { .. }) {
            0.1
        } else {
            1.0
        };
        if x < 0.0 || y < 0.0 || width < minimum_size || height < minimum_size {
            return Err(invalid(
                "A label item is too small or sits outside the label.",
            ));
        }
        if x + width > input.width_mm + EPSILON_MM || y + height > input.height_mm + EPSILON_MM {
            return Err(invalid(
                "A label item sticks out past the edge of the label.",
            ));
        }

        match element {
            LabelElement::Text {
                source,
                custom_text,
                font_size_pt,
                ..
            } => {
                check_font(*font_size_pt)?;
                match (source, custom_text) {
                    (TextSource::CustomText, Some(text)) => {
                        if text.trim().is_empty() {
                            return Err(invalid("A custom text item cannot be empty."));
                        }
                        if text.chars().count() > MAX_CUSTOM_TEXT_CHARS {
                            return Err(invalid(format!(
                                "Custom text can be at most {MAX_CUSTOM_TEXT_CHARS} characters."
                            )));
                        }
                    }
                    (TextSource::CustomText, None) => {
                        return Err(invalid("A custom text item needs its text."));
                    }
                    (_, Some(_)) => {
                        return Err(invalid("Only a custom text item can carry its own text."));
                    }
                    (_, None) => {}
                }
            }
            LabelElement::Barcode {
                height_mm,
                show_value,
                font_size_pt,
                ..
            } => {
                check_font(*font_size_pt)?;
                let caption = if *show_value {
                    caption_height_mm(*font_size_pt)
                } else {
                    0.0
                };
                if height_mm - caption < MIN_BARCODE_BARS_MM - EPSILON_MM {
                    return Err(invalid(
                        "A barcode is too short to scan. Make it taller or its number smaller.",
                    ));
                }
            }
            LabelElement::Qr { .. } | LabelElement::Line { .. } => {}
        }
    }
    Ok(())
}

fn check_font(font_size_pt: f64) -> AppResult<()> {
    if finite_in(font_size_pt, MIN_FONT_PT, MAX_FONT_PT) {
        Ok(())
    } else {
        Err(invalid(format!(
            "Text size must be between {MIN_FONT_PT} and {MAX_FONT_PT} pt."
        )))
    }
}

/// Space the template reserves under a barcode's bars for the printed number
/// (`(font * 1.25 + 2) pt`, mirroring `lbl_temp_*.typ`).
fn caption_height_mm(font_size_pt: f64) -> f64 {
    (font_size_pt * 1.25 + 2.0) * PT_TO_MM
}

/// Width of a Code128 symbol in modules (bars and spaces, quiet zones
/// included), estimated for `code`. All-digit codes pair up into compact
/// symbols; anything else costs one symbol per character. Measured against
/// Zint's real output (within 4% for digits, never lower than real for text),
/// then padded 5% so a design is never accepted that the printer cannot read.
pub(crate) fn code128_modules(code: &str) -> u32 {
    let length = code.chars().count() as u32;
    let all_digits = length >= 2 && code.chars().all(|c| c.is_ascii_digit());
    let symbols = if all_digits {
        if length.is_multiple_of(2) {
            length / 2
        } else {
            (length - 1) / 2 + 2
        }
    } else {
        length
    };
    // start + data + check symbols (11 modules each), the 13-module stop, 2 x 10 quiet.
    let modules = 11 * (symbols + 2) + 13 + 20;
    (f64::from(modules) * 1.05).ceil() as u32
}

/// Size of a QR code of `code` in modules (quiet zone included): the smallest
/// version holding it at error correction M in byte mode (what the template
/// draws), which over-counts numeric codes and so stays on the safe side.
pub(crate) fn qr_modules(code: &str) -> u32 {
    const BYTE_CAPACITY_M: [usize; 5] = [14, 26, 42, 62, 84];
    let length = code.len();
    let version = BYTE_CAPACITY_M
        .iter()
        .position(|capacity| *capacity >= length)
        .map_or(BYTE_CAPACITY_M.len(), |index| index + 1) as u32;
    17 + 4 * version + 4
}

/// Fails when a barcode or QR element of `elements` would print its `code`
/// with bars or dots thinner than a label printer can reproduce.
pub(crate) fn check_code_fits(
    elements: &[LabelElement],
    code: &str,
    product_name: &str,
) -> AppResult<()> {
    for element in elements {
        match element {
            LabelElement::Barcode { width_mm, .. } => {
                let needed = f64::from(code128_modules(code)) * MIN_MODULE_MM;
                if *width_mm + EPSILON_MM < needed {
                    return Err(AppError::unprocessable_entity(
                        "LABEL_BARCODE_TOO_NARROW",
                        format!(
                            "The barcode for '{product_name}' is too narrow to scan. Make it at least {:.0} mm wide, or use a QR code.",
                            needed.ceil()
                        ),
                    ));
                }
            }
            LabelElement::Qr {
                width_mm,
                height_mm,
                ..
            } => {
                let needed = f64::from(qr_modules(code)) * MIN_MODULE_MM;
                if width_mm.min(*height_mm) + EPSILON_MM < needed {
                    return Err(AppError::unprocessable_entity(
                        "LABEL_QR_TOO_SMALL",
                        format!(
                            "The QR code for '{product_name}' is too small to scan. Make it at least {:.0} mm wide and tall.",
                            needed.ceil()
                        ),
                    ));
                }
            }
            LabelElement::Text { .. } | LabelElement::Line { .. } => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
            message(validate_template(&input(vec![barcode(46.0, 5.9, true)])))
                .contains("too short")
        );
        assert!(validate_template(&input(vec![barcode(46.0, 3.0, false)])).is_ok());
        assert!(
            message(validate_template(&input(vec![barcode(46.0, 2.9, false)])))
                .contains("too short")
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
}
