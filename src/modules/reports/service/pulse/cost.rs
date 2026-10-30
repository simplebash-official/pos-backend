pub(super) fn cost(estimate: Option<i64>, material: Option<i64>) -> i64 {
    estimate.unwrap_or(0) + material.unwrap_or(0)
}
