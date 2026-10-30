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
