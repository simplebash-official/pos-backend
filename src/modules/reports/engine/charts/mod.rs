// High-speed Chart & Analytics Vectorizer.
// Precomputes cumulative curves, moving averages, top-slice normalization,
// and trend metrics in Rust in microseconds.

mod analyze_sales_patterns;
mod enrich_timeseries;
mod enriched_time_series_point;
mod top_categories_with_other;

pub use analyze_sales_patterns::analyze_sales_patterns;
pub use enrich_timeseries::enrich_timeseries;
pub use enriched_time_series_point::EnrichedTimeSeriesPoint;
pub use top_categories_with_other::top_categories_with_other;
