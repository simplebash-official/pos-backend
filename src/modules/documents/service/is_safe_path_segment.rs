/// True for a non-empty, at most 128-character string made only of ASCII
/// letters, digits, `_` and `-` — the alphabet of generated keys
/// (`<prefix>_<nanoid>`) and of the fixed cache keys. Anything else (a `/`,
/// `..`, a drive letter, a NUL) is refused before it can become a path.
pub(super) fn is_safe_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 128
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
