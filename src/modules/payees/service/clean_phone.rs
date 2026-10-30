/// A blank phone means none.
pub(super) fn clean_phone(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}
