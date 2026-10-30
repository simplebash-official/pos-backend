// Reports, Analytics & Chart Engine.
// Coordinates high-performance in-memory caching, singleflight coalescing,
// parallel multi-pipeline execution, and automated cache invalidation.

mod analytics_engine;
pub mod cache;
pub mod charts;
pub mod feed;
pub mod indexes;
pub mod singleflight;

pub use analytics_engine::AnalyticsEngine;
