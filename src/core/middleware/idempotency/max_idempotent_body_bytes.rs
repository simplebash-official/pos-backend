/// Largest request body this middleware will buffer (to hash it and replay
/// it to the handler). Matches axum's default `DefaultBodyLimit`; the only
/// route that legitimately takes more (`/api/backup`) is exempt below. A
/// bigger body is answered 413 without being read into memory.
pub const MAX_IDEMPOTENT_BODY_BYTES: usize = 2 * 1024 * 1024;
