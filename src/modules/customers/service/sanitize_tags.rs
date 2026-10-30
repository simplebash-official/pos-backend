pub(super) fn sanitize_tags(tags: Vec<String>) -> Vec<String> {
    let mut cleaned: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned
}
