/// A balance: where it started, plus what increased it, less what reduced it.
pub(super) fn owed(opening: i64, increases: i64, reductions: i64) -> i64 {
    opening + increases - reductions
}
