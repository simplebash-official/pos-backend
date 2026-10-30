// High-performance request coalescing / single-flight group for Tokio.
// Prevents duplicate database aggregation runs when multiple concurrent requests
// ask for the same period report at the exact same instant.

mod single_flight_group;
#[cfg(test)]
mod tests;

pub use single_flight_group::SingleFlightGroup;
