//! Register session lifecycle service: open, blind count, close, reopen, and variance conversion.

mod service_session;

pub use service_session::SHOP_DRAWER_KEY;
pub use service_session::active_drawer_key;
pub use service_session::cancel_close;
pub use service_session::close_session;
pub use service_session::convert_variance;
pub use service_session::current_shift_mode;
pub use service_session::current_shift_policy;
pub(crate) use service_session::expected_opening_float;
pub use service_session::get_active_session_for_cashier;
pub use service_session::get_active_sessions_summary;
pub use service_session::get_session;
pub use service_session::list_sessions;
pub use service_session::open_session;
pub use service_session::reopen_session;
pub use service_session::resolve_active_session;
pub use service_session::resolve_writable_session;
pub use service_session::submit_blind_count;
