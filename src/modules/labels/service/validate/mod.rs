// Rules a label design must satisfy, and the "will this scan?" checks run
// against the real codes at print time. Pure functions: no I/O.

mod check_code_fits;
mod code128_modules;
mod epsilon_mm;
mod max_custom_text_chars;
mod max_elements;
mod max_font_pt;
mod max_height_mm;
mod max_id_chars;
mod max_name_chars;
mod max_width_mm;
mod min_font_pt;
mod min_height_mm;
mod min_module_mm;
mod min_width_mm;
mod qr_modules;
#[cfg(test)]
mod tests;
mod validate_template;

pub(crate) use check_code_fits::check_code_fits;

pub(crate) use validate_template::validate_template;
