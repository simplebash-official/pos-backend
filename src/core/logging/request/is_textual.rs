pub(super) fn is_textual(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|ct| {
        let ct = ct.to_ascii_lowercase();
        ct.starts_with("application/json")
            || ct.starts_with("text/")
            || ct.starts_with("application/x-www-form-urlencoded")
    })
}
