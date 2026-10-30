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
