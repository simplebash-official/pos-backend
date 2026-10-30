//! Register session lifecycle service: open, blind count, close, reopen, and variance conversion.

mod service_session;

pub use service_session::cancel_close;
pub use service_session::close_session;
pub use service_session::convert_variance;
pub(crate) use service_session::expected_opening_float;
pub use service_session::get_active_session_for_cashier;
pub use service_session::get_active_session_for_terminal;
pub use service_session::get_session;
pub use service_session::list_sessions;
pub use service_session::open_session;
pub use service_session::populate_missing_cashier_name;
pub use service_session::reopen_session;
pub use service_session::resolve_active_session;
pub use service_session::submit_blind_count;
