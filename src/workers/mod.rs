//! Background jobs that run for the life of the process.

mod notification_retention;

pub use notification_retention::run_notification_retention;
