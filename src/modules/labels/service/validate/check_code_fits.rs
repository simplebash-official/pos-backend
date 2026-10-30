use super::code128_modules::code128_modules;
use super::epsilon_mm::EPSILON_MM;
use super::min_module_mm::MIN_MODULE_MM;
use super::qr_modules::qr_modules;
use crate::core::error::{AppError, AppResult};
use crate::domain::labels::LabelElement;

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
