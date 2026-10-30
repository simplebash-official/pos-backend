//! The application error type and its HTTP mapping.

mod app_error;
mod core_error;

pub use app_error::AppError;
pub use core_error::AppResult;
pub use core_error::WireParts;
