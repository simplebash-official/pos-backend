//! High-speed Chart & Analytics Vectorizer.
//! Precomputes cumulative curves, moving averages, top-slice normalization,
//! and trend metrics in Rust in microseconds.

mod engine_charts;

pub use engine_charts::EnrichedTimeSeriesPoint;
pub use engine_charts::analyze_sales_patterns;
pub use engine_charts::enrich_timeseries;
pub use engine_charts::top_categories_with_other;
