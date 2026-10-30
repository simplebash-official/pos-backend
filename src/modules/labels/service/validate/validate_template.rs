use super::epsilon_mm::EPSILON_MM;
use super::max_custom_text_chars::MAX_CUSTOM_TEXT_CHARS;
use super::max_elements::MAX_ELEMENTS;
use super::max_font_pt::MAX_FONT_PT;
use super::max_height_mm::MAX_HEIGHT_MM;
use super::max_id_chars::MAX_ID_CHARS;
use super::max_name_chars::MAX_NAME_CHARS;
use super::max_width_mm::MAX_WIDTH_MM;
use super::min_font_pt::MIN_FONT_PT;
use super::min_height_mm::MIN_HEIGHT_MM;
use super::min_width_mm::MIN_WIDTH_MM;
use crate::core::error::{AppError, AppResult};
use crate::domain::labels::{LabelElement, LabelFrame, LabelTemplateInput, TextSource};

/// A barcode's bars must stay this tall (excluding its caption) to be read.
const MIN_BARCODE_BARS_MM: f64 = 3.0;

const PT_TO_MM: f64 = 25.4 / 72.0;

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
        let LabelFrame {
            id,
            x_mm: x,
            y_mm: y,
            width_mm: width,
            height_mm: height,
        } = element.frame();
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
