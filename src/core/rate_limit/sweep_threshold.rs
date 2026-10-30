/// Above this many tracked keys, expired entries are swept on the next write
/// so a stream of random keys can't grow the map without bound.
pub(super) const SWEEP_THRESHOLD: usize = 10_000;
