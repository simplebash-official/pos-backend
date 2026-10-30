//! The opaque pull cursor handed to devices.

mod sync_cursor;

pub use sync_cursor::DecodedCursor;
pub use sync_cursor::TOMBSTONE_RETENTION_DAYS;
pub use sync_cursor::decode_cursor;
pub use sync_cursor::encode_cursor;
