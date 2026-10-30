/// Spreads `amount_cents` (signed) over lines in proportion to `weights`, to the cent: each line
/// first gets its whole-cent share, then the cents left over go one each to the lines with the
/// largest fractional remainders (ties to the earlier line). The shares always add up to the
/// amount. The weights must be positive and not empty.
#[must_use]
pub fn allocate(amount_cents: i64, weights: &[i64]) -> Vec<i64> {
    let total_weight: i128 = weights.iter().map(|&w| i128::from(w)).sum();
    debug_assert!(total_weight > 0, "weights must add up to something");
    let magnitude = i128::from(amount_cents.unsigned_abs());

    let mut shares: Vec<i128> = Vec::with_capacity(weights.len());
    let mut remainders: Vec<(i128, usize)> = Vec::with_capacity(weights.len());
    for (index, &weight) in weights.iter().enumerate() {
        let scaled = magnitude * i128::from(weight);
        shares.push(scaled / total_weight);
        remainders.push((scaled % total_weight, index));
    }
    let leftover = magnitude - shares.iter().sum::<i128>();
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, index) in remainders.iter().take(leftover as usize) {
        shares[index] += 1;
    }

    let sign: i128 = if amount_cents < 0 { -1 } else { 1 };
    shares
        .into_iter()
        .map(|share| (share * sign) as i64)
        .collect()
}
