use crate::core::calculations::share_basis_points;

pub(super) fn percent(count: i64, total: i64) -> i64 {
    (share_basis_points(count, total) + 50) / 100
}
