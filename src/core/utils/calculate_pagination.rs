/// Computes pagination parameters `(page, limit, skip)` with default bounds.
#[must_use]
pub fn calculate_pagination(
    page: Option<u64>,
    limit: Option<u64>,
    default_limit: u64,
    max_limit: u64,
) -> (u64, u64, u64) {
    let page = page.unwrap_or(1).max(1);
    let limit = limit.unwrap_or(default_limit).clamp(1, max_limit);
    let skip = (page - 1) * limit;
    (page, limit, skip)
}
