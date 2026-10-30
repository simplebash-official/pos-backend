use crate::core::error::{AppError, AppResult};

use crate::domain::labels::{CreateLabelJobRequest, LabelPrintAction, LabelPrintMethod};

/// The printer resolutions the print setup offers.
const DPI_OPTIONS: [u32; 3] = [203, 300, 600];

const MAX_PRINTER_NAME_CHARS: usize = 200;

/// A print names how it leaves the computer; a download names nothing.
pub(super) fn validate_job_request(request: &CreateLabelJobRequest) -> AppResult<()> {
    match request.action {
        LabelPrintAction::Download => {
            if request.method.is_some() || request.printer_name.is_some() || request.dpi.is_some() {
                return Err(AppError::validation(
                    "A download has no printer, print method or resolution.",
                ));
            }
        }
        LabelPrintAction::Print => {
            let Some(method) = request.method else {
                return Err(AppError::validation("Choose how the labels print."));
            };
            match request.dpi {
                Some(dpi) if DPI_OPTIONS.contains(&dpi) => {}
                _ => {
                    return Err(AppError::validation(
                        "Choose the printer's resolution: 203, 300 or 600 dpi.",
                    ));
                }
            }
            let printer = request.printer_name.as_deref().map(str::trim);
            match (method, printer) {
                (LabelPrintMethod::Direct, Some(name))
                    if !name.is_empty() && name.chars().count() <= MAX_PRINTER_NAME_CHARS => {}
                (LabelPrintMethod::Direct, _) => {
                    return Err(AppError::validation("Choose the printer to print on."));
                }
                (LabelPrintMethod::System, None) => {}
                (LabelPrintMethod::System, Some(_)) => {
                    return Err(AppError::validation(
                        "The system print window chooses its own printer.",
                    ));
                }
            }
        }
    }
    Ok(())
}
