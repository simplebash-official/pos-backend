//! Request and response headers used by device sync: device id, `If-Match` and server time.

mod middleware_sync_headers;

pub use middleware_sync_headers::DeviceId;
pub use middleware_sync_headers::IfMatch;
pub use middleware_sync_headers::add_server_time_header;
