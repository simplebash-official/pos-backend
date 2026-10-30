//! The dashboard's groupings and totals: the repair and print pipelines, technician load, what is
//! selling today, what is low or out of stock, repairs waiting for pickup and the activity stream.
//! `compute` is pure; `get_dashboard_pulse` reads the records it needs. Wording, colours and
//! "time ago" stay on the screen: this sends figures, codes and dates.

mod compute;
mod service_pulse;
#[cfg(test)]
mod tests;

pub use service_pulse::get_dashboard_pulse;
