//! Rules a label design must satisfy, and the "will this scan?" checks run
//! against the real codes at print time. Pure functions: no I/O.

mod service_validate;
#[cfg(test)]
mod tests;

pub(crate) use service_validate::check_code_fits;

pub(crate) use service_validate::validate_template;
